use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail, ensure};
use reqwest::Url;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub schema_version: u32,
    pub url: String,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

impl Source {
    pub fn validate(&self, allow_http: bool) -> Result<Url> {
        ensure!(
            self.schema_version == 1,
            "unsupported source schema version"
        );
        ensure!(
            self.sha256.len() == 64 && self.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "SHA-256 must contain exactly 64 hexadecimal characters"
        );
        let url = Url::parse(&self.url).context("invalid source URL")?;
        ensure!(
            url.scheme() == "https" || (allow_http && url.scheme() == "http"),
            "only HTTPS is allowed (HTTP requires --allow-http)"
        );
        ensure!(
            url.username().is_empty() && url.password().is_none(),
            "URL credentials are not supported"
        );
        ensure!(url.fragment().is_none(), "URL fragments are not supported");
        Ok(url)
    }
}

#[derive(Debug, Serialize)]
pub struct FetchResult {
    pub output: PathBuf,
    pub bytes: u64,
    pub sha256: String,
    pub reused: bool,
}

pub fn digest_file(path: &Path) -> Result<(String, u64)> {
    let mut file = File::open(path).context("opening file for verification")?;
    hash_stream(&mut file, &mut std::io::sink(), None)
}

fn hash_stream(
    input: &mut impl Read,
    output: &mut impl Write,
    maximum: Option<u64>,
) -> Result<(String, u64)> {
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 256 * 1024];
    let mut bytes = 0_u64;
    loop {
        let count = input.read(&mut buffer).context("reading artifact data")?;
        if count == 0 {
            break;
        }
        bytes = bytes
            .checked_add(count as u64)
            .context("artifact size overflow")?;
        if let Some(maximum) = maximum {
            ensure!(bytes <= maximum, "artifact exceeds declared size");
        }
        digest.update(&buffer[..count]);
        output.write_all(&buffer[..count])?;
    }
    Ok((format!("{:x}", digest.finalize()), bytes))
}

fn verify(source: &Source, digest: &str, bytes: u64) -> Result<()> {
    ensure!(
        source.sha256.eq_ignore_ascii_case(digest),
        "SHA-256 mismatch: expected {}, received {}",
        source.sha256,
        digest
    );
    if let Some(expected) = source.size {
        ensure!(
            bytes == expected,
            "size mismatch: expected {expected}, received {bytes}"
        );
    }
    Ok(())
}

fn verify_existing(source: &Source, path: &Path) -> Result<(String, u64)> {
    ensure!(
        fs::symlink_metadata(path)?.file_type().is_file(),
        "existing output must be a regular file, not a link"
    );
    let (digest, bytes) = digest_file(path)?;
    verify(source, &digest, bytes)
        .context("existing output is invalid; refusing to overwrite it")?;
    Ok((digest, bytes))
}

pub fn fetch(
    source: &Source,
    output: &Path,
    allow_http: bool,
    timeout: Duration,
) -> Result<FetchResult> {
    let url = source.validate(allow_http)?;
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).context("creating output directory")?;
    match fs::symlink_metadata(output) {
        Ok(_) => {
            let (digest, bytes) = verify_existing(source, output)?;
            write_sidecar(source, output)?;
            return Ok(FetchResult {
                output: output.into(),
                bytes,
                sha256: digest,
                reused: true,
            });
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let client = Client::builder()
        .https_only(!allow_http)
        .connect_timeout(Duration::from_secs(30))
        .timeout(timeout)
        .user_agent(concat!("oci-kit/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let mut response = client
        .get(url)
        .send()
        .map_err(|error| anyhow!(error.without_url()))?
        .error_for_status()
        .map_err(|error| anyhow!(error.without_url()))?;
    if let (Some(declared), Some(received)) = (source.size, response.content_length()) {
        ensure!(
            declared == received,
            "HTTP Content-Length disagrees with declared size"
        );
    }
    let mut temporary = NamedTempFile::new_in(parent).context("creating temporary download")?;
    let (digest, bytes) = hash_stream(&mut response, &mut temporary, source.size)?;
    verify(source, &digest, bytes)?;
    temporary.as_file().sync_all()?;
    let reused = match temporary.persist_noclobber(output) {
        Ok(_) => false,
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            verify_existing(source, output)?;
            true
        }
        Err(error) => bail!("persisting verified artifact: {}", error.error),
    };
    write_sidecar(source, output)?;
    Ok(FetchResult {
        output: output.into(),
        bytes,
        sha256: digest,
        reused,
    })
}

fn write_sidecar(source: &Source, output: &Path) -> Result<()> {
    let mut sidecar_name = output.as_os_str().to_owned();
    sidecar_name.push(".source.json");
    let sidecar = PathBuf::from(sidecar_name);
    let parent = sidecar
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temporary, source)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary.persist(sidecar).map_err(|error| error.error)?;
    Ok(())
}
