use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;

const MAX_TEXT_BYTES: u64 = 64 * 1024;

#[derive(Debug, Serialize)]
pub struct EntryInfo {
    pub path: PathBuf,
    pub kind: String,
    pub size: u64,
    pub uid: u64,
    pub gid: u64,
    pub mode: u32,
    pub link_target: Option<PathBuf>,
}

#[derive(Debug, Default, Serialize)]
pub struct ArchiveReport {
    pub entries: u64,
    pub logical_bytes: u64,
    pub kinds: BTreeMap<String, u64>,
    pub top_level_bytes: BTreeMap<String, u64>,
    pub selected: Vec<EntryInfo>,
    pub text: BTreeMap<PathBuf, String>,
}

fn archive_path(path: &Path) -> Result<PathBuf> {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => result.push(name),
            Component::CurDir => {}
            _ => bail!("unsafe archive entry path: {}", path.display()),
        }
    }
    Ok(result)
}

fn reader(path: &Path) -> Result<Box<dyn Read>> {
    let mut input = BufReader::new(File::open(path).context("opening archive")?);
    let magic = input.fill_buf()?;
    if magic.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        Ok(Box::new(zstd::stream::read::Decoder::new(input)?))
    } else if magic.starts_with(&[0x1f, 0x8b]) {
        Ok(Box::new(flate2::read::MultiGzDecoder::new(input)))
    } else {
        Ok(Box::new(input))
    }
}

pub fn inspect_archive(
    path: &Path,
    selections: &[PathBuf],
    texts: &[PathBuf],
) -> Result<ArchiveReport> {
    let selections: Vec<_> = selections
        .iter()
        .map(|path| archive_path(path))
        .collect::<Result<_>>()?;
    let texts: Vec<_> = texts
        .iter()
        .map(|path| archive_path(path))
        .collect::<Result<_>>()?;
    let mut archive = tar::Archive::new(reader(path)?);
    let mut report = ArchiveReport::default();
    for entry in archive.entries()? {
        let entry = entry.context("reading tar entry")?;
        let path = archive_path(&entry.path()?)?;
        if path.as_os_str().is_empty() {
            continue;
        }
        let header = entry.header();
        let kind = match header.entry_type() {
            kind if kind.is_dir() => "directory",
            kind if kind.is_file() || kind.is_gnu_sparse() => "file",
            kind if kind.is_symlink() => "symlink",
            kind if kind.is_hard_link() => "hardlink",
            kind if kind.is_block_special() || kind.is_character_special() => "device",
            kind if kind.is_fifo() => "fifo",
            _ => "other",
        };
        let info = EntryInfo {
            path: path.clone(),
            kind: kind.into(),
            size: entry.size(),
            uid: header.uid()?,
            gid: header.gid()?,
            mode: header.mode()?,
            link_target: entry.link_name()?.map(|target| target.into_owned()),
        };
        report.entries += 1;
        report.logical_bytes = report
            .logical_bytes
            .checked_add(info.size)
            .context("archive size overflow")?;
        *report.kinds.entry(kind.into()).or_default() += 1;
        let top = path
            .components()
            .next()
            .context("missing top-level component")?
            .as_os_str()
            .to_string_lossy()
            .into_owned();
        let top_bytes = report.top_level_bytes.entry(top).or_default();
        *top_bytes = top_bytes
            .checked_add(info.size)
            .context("archive size overflow")?;
        if texts.contains(&path) {
            ensure!(
                kind == "file",
                "requested text is not a regular file: {}",
                path.display()
            );
            ensure!(
                info.size <= MAX_TEXT_BYTES,
                "requested text exceeds 64 KiB: {}",
                path.display()
            );
            let mut content = String::new();
            entry
                .take(MAX_TEXT_BYTES + 1)
                .read_to_string(&mut content)
                .context("requested text is not valid UTF-8")?;
            ensure!(content.len() as u64 <= MAX_TEXT_BYTES, "text exceeds limit");
            report.text.insert(path.clone(), content);
        }
        if selections.iter().any(|prefix| path.starts_with(prefix)) {
            report.selected.push(info);
        }
    }
    Ok(report)
}
