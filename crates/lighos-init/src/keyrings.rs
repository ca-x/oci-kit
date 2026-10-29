use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, ensure};
use oci_kit::staging::StagingRoot;
use serde::Serialize;

use crate::Profile;

#[derive(Serialize)]
pub struct KeyringPlan {
    pub root: PathBuf,
    pub pacman_keyrings: Vec<String>,
    pub apply: bool,
}

pub fn prepare_keyrings(root: &Path, profile: &Profile, should_apply: bool) -> Result<KeyringPlan> {
    let root = StagingRoot::open(root)?;
    for relative in [
        "var/lib/lightos/initialized",
        "etc/lightos/seed.env",
        "etc/lightos/remote-desktop.env",
    ] {
        ensure!(
            root.kind(Path::new(relative))?.is_none(),
            "offline keyring preparation refuses platform or remote state"
        );
    }
    ensure!(
        root.kind(Path::new("usr/bin/pacman-key"))? == Some("file"),
        "offline keyring preparation requires native pacman-key"
    );
    let mut keyrings = vec!["archlinux".to_owned()];
    for keyring in &profile.pacman_keyrings {
        if !keyrings.contains(keyring) {
            keyrings.push(keyring.clone());
        }
    }
    for keyring in &keyrings {
        let path = PathBuf::from(format!("usr/share/pacman/keyrings/{keyring}.gpg"));
        let bytes = root.read_file(&path, 32 * 1024 * 1024)?;
        ensure!(
            bytes.is_some_and(|bytes| !bytes.is_empty()),
            "configured pacman keyring is absent: {keyring}"
        );
    }
    let plan = KeyringPlan {
        root: root.path().to_path_buf(),
        pacman_keyrings: keyrings,
        apply: should_apply,
    };
    if should_apply {
        ensure!(
            rustix::process::geteuid().is_root(),
            "offline keyring preparation requires root"
        );
        run(&root, &["--init"])?;
        for keyring in &plan.pacman_keyrings {
            let path = format!("/usr/share/pacman/keyrings/{keyring}.gpg");
            run(&root, &["--add", &path])?;
            run(&root, &["--populate", keyring])?;
        }
    }
    Ok(plan)
}

fn run(root: &StagingRoot, arguments: &[&str]) -> Result<()> {
    let status = Command::new("/usr/bin/bwrap")
        .env_clear()
        .args([
            "--unshare-pid",
            "--unshare-ipc",
            "--unshare-net",
            "--unshare-uts",
            "--die-with-parent",
            "--new-session",
            "--bind",
        ])
        .arg(root.process_fd_path())
        .args([
            "/",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--clearenv",
            "--setenv",
            "PATH",
            "/usr/bin:/usr/sbin",
            "--",
            "/usr/bin/pacman-key",
        ])
        .args(arguments)
        .stdout(std::io::stderr())
        .status()
        .context("running offline pacman-key in the staged namespace")?;
    ensure!(
        status.success(),
        "offline pacman-key failed ({})",
        arguments[0]
    );
    Ok(())
}
