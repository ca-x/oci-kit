use std::fs::{self, File, Metadata};
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, ensure};
use oci_kit::staging::StagingRoot;
use rustix::fs::{self as sys, Mode, OFlags, ResolveFlags};
use rustix::io::Errno;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

const UNIT: &str = include_str!("../systemd/lighos-init.service");
const STATE_DIRECTORY: &str = "/var/lib/oci-kit";
const STATE_FILE: &str = "/var/lib/oci-kit/lightos-init.json";
const PLATFORM_MARKER: &str = "/var/lib/lightos/initialized";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub schema_version: u32,
    #[serde(default)]
    pub pacman_keyrings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub post_setup_services: Vec<String>,
}

impl Profile {
    pub fn parse(input: &str) -> Result<Self> {
        let profile: Self = toml::from_str(input).context("invalid LightOS profile")?;
        ensure!(
            profile.schema_version == 1,
            "unsupported LightOS profile schema"
        );
        for name in &profile.pacman_keyrings {
            ensure!(
                name.bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_alphanumeric())
                    && name.len() <= 64
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte)),
                "invalid pacman keyring name"
            );
        }
        ensure!(
            profile.post_setup_services.len() <= 32,
            "too many platform services"
        );
        let mut services = BTreeSet::new();
        for name in &profile.post_setup_services {
            ensure!(
                name.ends_with(".service")
                    && name.len() > ".service".len()
                    && name.len() <= 128
                    && name
                        .bytes()
                        .next()
                        .is_some_and(|byte| byte.is_ascii_alphanumeric())
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"_.-@".contains(&byte)),
                "invalid platform service name"
            );
            ensure!(
                !matches!(name.as_str(), "lighos-init.service" | "lightos-setup.service"),
                "platform service creates an initialization dependency cycle"
            );
            ensure!(services.insert(name), "duplicate platform service");
        }
        Ok(profile)
    }

    pub fn fingerprint(&self) -> Result<String> {
        let mut digest = Sha256::new();
        digest.update(env!("CARGO_PKG_VERSION"));
        digest.update(serde_json::to_vec(self)?);
        Ok(format!("{:x}", digest.finalize()))
    }
}

#[derive(Debug, Serialize)]
pub struct InitPlan {
    pub platform: &'static str,
    pub fingerprint: String,
    pub after: &'static str,
    pub pacman_keyrings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub post_setup_services: Vec<String>,
    pub apply: bool,
}

pub fn plan(profile: &Profile, apply: bool) -> Result<InitPlan> {
    Ok(InitPlan {
        platform: "lightos",
        fingerprint: profile.fingerprint()?,
        after: PLATFORM_MARKER,
        pacman_keyrings: profile.pacman_keyrings.clone(),
        post_setup_services: profile.post_setup_services.clone(),
        apply,
    })
}

pub fn install(root: &Path, profile: &Profile, binary: &Path) -> Result<()> {
    let root = StagingRoot::open(root)?;
    root.write_file(
        Path::new("usr/local/bin/lighos-init"),
        &fs::read(binary)?,
        0o755,
    )?;
    root.write_file(
        Path::new("etc/oci-kit/lightos.toml"),
        toml::to_string(profile)?.as_bytes(),
        0o644,
    )?;
    root.write_file(
        Path::new("etc/systemd/system/lighos-init.service"),
        UNIT.as_bytes(),
        0o644,
    )?;
    for name in &profile.post_setup_services {
        root.write_file(
            &std::path::PathBuf::from(format!(
                "etc/systemd/system/{name}.d/20-oci-kit-post-setup.conf"
            )),
            b"[Unit]\nRequires=lighos-init.service\nAfter=lighos-init.service\n",
            0o644,
        )?;
    }
    root.symlink(
        Path::new("etc/systemd/system/multi-user.target.wants/lighos-init.service"),
        Path::new("../lighos-init.service"),
    )?;
    Ok(())
}

pub fn apply(profile: &Profile) -> Result<bool> {
    ensure!(
        rustix::process::geteuid().is_root(),
        "LightOS initialization requires root"
    );
    apply_at(Path::new("/"), profile, |keyring| {
        let keyfile = format!("/usr/share/pacman/keyrings/{keyring}.gpg");
        for arguments in [["--add", keyfile.as_str()], ["--populate", keyring]] {
            let status = Command::new("/usr/bin/pacman-key")
                .args(arguments)
                .env("PATH", "/usr/bin:/usr/sbin")
                .stdout(std::io::stderr())
                .status()
                .context("running pacman-key")?;
            ensure!(
                status.success(),
                "pacman-key failed for {keyring} ({})",
                arguments[0]
            );
        }
        Ok(())
    })
}

fn trusted_metadata(metadata: &Metadata) -> Result<()> {
    ensure!(
        metadata.uid() == rustix::process::geteuid().as_raw() && metadata.mode() & 0o022 == 0,
        "initialization state must be owned by the runtime user and not group/world-writable"
    );
    Ok(())
}

fn open_state_directory(root: &Path) -> Result<File> {
    let mut directory = File::from(
        sys::openat2(
            sys::CWD,
            root,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
            ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_MAGICLINKS,
        )
        .context("opening initialization state root")?,
    );
    trusted_metadata(&directory.metadata()?)?;
    for component in Path::new(STATE_DIRECTORY.trim_start_matches('/')).components() {
        match sys::mkdirat(&directory, component.as_os_str(), Mode::RWXU) {
            Ok(()) | Err(Errno::EXIST) => {}
            Err(error) => return Err(error.into()),
        }
        directory = File::from(
            sys::openat(
                &directory,
                component.as_os_str(),
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .context("opening trusted initialization state directory")?,
        );
        trusted_metadata(&directory.metadata()?)?;
    }
    Ok(directory)
}

fn open_state_file(directory: &File, name: &str, flags: OFlags) -> Result<File> {
    match sys::openat(
        directory,
        name,
        OFlags::PATH | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(descriptor) => {
            let metadata = File::from(descriptor).metadata()?;
            ensure!(
                metadata.is_file(),
                "initialization state and lock must be a regular file"
            );
            trusted_metadata(&metadata)?;
        }
        Err(Errno::NOENT) => {}
        Err(error) => return Err(error.into()),
    }
    let file = File::from(sys::openat(
        directory,
        name,
        flags | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        Mode::RUSR | Mode::WUSR,
    )?);
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file(),
        "initialization state and lock must be a regular file"
    );
    trusted_metadata(&metadata)?;
    Ok(file)
}

fn apply_at(
    root: &Path,
    profile: &Profile,
    mut populate: impl FnMut(&str) -> Result<()>,
) -> Result<bool> {
    let platform_marker = root.join(PLATFORM_MARKER.trim_start_matches('/'));
    let state_directory = root.join(STATE_DIRECTORY.trim_start_matches('/'));
    let state_file = root.join(STATE_FILE.trim_start_matches('/'));
    ensure!(
        platform_marker.is_file(),
        "LightOS account setup has not completed; refusing to apply"
    );
    let directory = open_state_directory(root)?;
    let lock = open_state_file(
        &directory,
        "lightos-init.lock",
        OFlags::RDWR | OFlags::CREATE,
    )?;
    lock.try_lock()
        .context("another LightOS initialization is running")?;
    let fingerprint = profile.fingerprint()?;
    match open_state_file(&directory, "lightos-init.json", OFlags::RDONLY) {
        Ok(file) => {
            let mut state = String::new();
            file.take(4097).read_to_string(&mut state)?;
            ensure!(state.len() <= 4096, "initialization state exceeds limit");
            let state: State =
                serde_json::from_str(&state).context("invalid initialization state")?;
            ensure!(
                state.schema_version == 1,
                "unsupported initialization state version"
            );
            if state.fingerprint == fingerprint {
                return Ok(false);
            }
        }
        Err(error) if error.downcast_ref::<Errno>() == Some(&Errno::NOENT) => {}
        Err(error) => return Err(error),
    }
    for keyring in &profile.pacman_keyrings {
        let path = root.join(format!("usr/share/pacman/keyrings/{keyring}.gpg"));
        ensure!(
            path.is_file(),
            "configured pacman keyring is absent: {keyring}"
        );
    }
    for keyring in &profile.pacman_keyrings {
        populate(keyring)?;
    }
    let mut temporary = NamedTempFile::new_in(&state_directory)?;
    serde_json::to_writer_pretty(
        &mut temporary,
        &State {
            schema_version: 1,
            fingerprint,
        },
    )?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary.persist(state_file).map_err(|error| error.error)?;
    directory.sync_all()?;
    Ok(true)
}

#[derive(Deserialize, Serialize)]
struct State {
    schema_version: u32,
    fingerprint: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_refuses_unknown_fields_and_command_injection() {
        for input in [
            "schema_version=2",
            "schema_version=1\npassword='not-allowed'",
            "schema_version=1\npacman_keyrings=['pond; reboot']",
            "schema_version=1\npacman_keyrings=['../escape']",
            "schema_version=1\npacman_keyrings=['--help']",
        ] {
            assert!(Profile::parse(input).is_err(), "accepted {input}");
        }
    }

    #[test]
    fn install_writes_only_to_marked_staging_and_is_repeatable() {
        let directory = tempfile::tempdir().unwrap();
        oci_kit::staging::mark(directory.path()).unwrap();
        let binary = directory.path().join("fixture-binary");
        fs::write(&binary, b"fixture executable").unwrap();
        let profile = Profile::parse("schema_version=1\npacman_keyrings=['pond']").unwrap();
        install(directory.path(), &profile, &binary).unwrap();
        install(directory.path(), &profile, &binary).unwrap();
        let installed =
            fs::read_to_string(directory.path().join("etc/oci-kit/lightos.toml")).unwrap();
        assert!(installed.contains("pond"));
        let unit = fs::read_to_string(
            directory
                .path()
                .join("etc/systemd/system/lighos-init.service"),
        )
        .unwrap();
        assert!(unit.contains("After=lightos-setup.service"));
        assert!(!unit.contains("seed.env"));
        assert_eq!(
            fs::read_link(
                directory
                    .path()
                    .join("etc/systemd/system/multi-user.target.wants/lighos-init.service")
            )
            .unwrap(),
            Path::new("../lighos-init.service")
        );
    }

    #[test]
    fn plan_is_deterministic_and_has_no_account_or_ssh_actions() {
        let profile = Profile::parse("schema_version=1\npacman_keyrings=['pond']").unwrap();
        assert_eq!(
            profile.fingerprint().unwrap(),
            profile.fingerprint().unwrap()
        );
        let plan = plan(&profile, false).unwrap();
        assert!(!plan.apply);
        assert_eq!(plan.platform, "lightos");
        assert_eq!(plan.pacman_keyrings, ["pond"]);
    }

    fn fixture() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir_all(directory.path().join("var/lib/lightos")).unwrap();
        fs::write(
            directory.path().join("var/lib/lightos/initialized"),
            b"initialized",
        )
        .unwrap();
        fs::create_dir_all(directory.path().join("usr/share/pacman/keyrings")).unwrap();
        fs::write(
            directory.path().join("usr/share/pacman/keyrings/pond.gpg"),
            b"fixture",
        )
        .unwrap();
        directory
    }

    #[test]
    fn success_is_idempotent_and_does_not_consume_platform_secrets() {
        let directory = fixture();
        fs::create_dir_all(directory.path().join("etc/lightos")).unwrap();
        let seed = directory.path().join("etc/lightos/seed.env");
        fs::write(&seed, b"opaque platform-owned secret").unwrap();
        let profile = Profile::parse("schema_version=1\npacman_keyrings=['pond']").unwrap();
        let mut calls = Vec::new();
        assert!(
            apply_at(directory.path(), &profile, |name| {
                calls.push(name.to_owned());
                Ok(())
            })
            .unwrap()
        );
        assert!(
            !apply_at(directory.path(), &profile, |_| panic!(
                "completed work must not rerun"
            ))
            .unwrap()
        );
        assert_eq!(calls, ["pond"]);
        assert_eq!(fs::read(seed).unwrap(), b"opaque platform-owned secret");
    }

    #[test]
    fn failed_command_leaves_no_success_state_and_can_be_retried() {
        let directory = fixture();
        let profile = Profile::parse("schema_version=1\npacman_keyrings=['pond']").unwrap();
        assert!(
            apply_at(directory.path(), &profile, |_| anyhow::bail!(
                "fixture failure"
            ))
            .is_err()
        );
        assert!(
            !directory
                .path()
                .join("var/lib/oci-kit/lightos-init.json")
                .exists()
        );
        assert!(apply_at(directory.path(), &profile, |_| Ok(())).unwrap());
    }

    #[test]
    fn requires_platform_completion_and_preflights_all_keyrings() {
        let directory = tempfile::tempdir().unwrap();
        let profile = Profile::parse("schema_version=1\npacman_keyrings=['pond']").unwrap();
        assert!(
            apply_at(directory.path(), &profile, |_| panic!(
                "no commands before platform setup"
            ))
            .is_err()
        );
        assert!(!directory.path().join("var/lib/oci-kit").exists());
        let directory = fixture();
        let profile =
            Profile::parse("schema_version=1\npacman_keyrings=['pond','missing']").unwrap();
        assert!(
            apply_at(directory.path(), &profile, |_| panic!(
                "no commands before preflight"
            ))
            .is_err()
        );
    }

    #[test]
    fn profile_change_invalidates_completion_state() {
        let directory = fixture();
        let empty = Profile::parse("schema_version=1").unwrap();
        assert!(apply_at(directory.path(), &empty, |_| Ok(())).unwrap());
        let populated = Profile::parse("schema_version=1\npacman_keyrings=['pond']").unwrap();
        let mut calls = 0;
        assert!(
            apply_at(directory.path(), &populated, |_| {
                calls += 1;
                Ok(())
            })
            .unwrap()
        );
        assert_eq!(calls, 1);
    }
}
