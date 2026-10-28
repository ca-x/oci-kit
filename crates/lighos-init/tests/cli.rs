use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use serde_json::Value;

const INIT: &str = env!("CARGO_BIN_EXE_lighos-init");

#[test]
fn help_version_and_deterministic_plan_are_available_without_root() {
    for flag in ["--help", "--version"] {
        let output = Command::new(INIT).arg(flag).output().unwrap();
        assert!(output.status.success());
        assert!(!output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("profile.toml");
    fs::write(&config, "schema_version=1\npacman_keyrings=['pond']").unwrap();
    let invoke = || {
        Command::new(INIT)
            .args(["apply", "--config"])
            .arg(&config)
            .output()
            .unwrap()
    };
    let first = invoke();
    let second = invoke();
    assert!(first.status.success());
    assert!(second.status.success());
    assert_eq!(first.stdout, second.stdout);
    let plan: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(plan["platform"], "lightos");
    assert_eq!(plan["apply"], false);
    assert_eq!(plan["pacman_keyrings"][0], "pond");
    assert_eq!(plan["after"], "/var/lib/lightos/initialized");
    assert!(plan.get("changed").is_none());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn failed_apply_emits_no_success_document() {
    if rustix::process::geteuid().is_root() {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("profile.toml");
    fs::write(&config, "schema_version=1\npacman_keyrings=['pond']").unwrap();
    let output = Command::new(INIT)
        .args(["apply", "--apply", "--config"])
        .arg(config)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires root"));
    assert!(output.stdout.is_empty(), "failure emitted a success plan");
}

#[test]
fn invalid_profile_is_rejected_before_install_or_apply() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("profile.toml");
    for profile in [
        "schema_version=2",
        "schema_version=1\npassword='not-a-supported-field'",
        "schema_version=1\npacman_keyrings=['../escape']",
        "schema_version=1\npacman_keyrings=['--help']",
        "schema_version=1\npacman_keyrings=['pond;whoami']",
    ] {
        fs::write(&config, profile).unwrap();
        for command in ["apply", "install"] {
            let mut invocation = Command::new(INIT);
            invocation.arg(command).arg("--config").arg(&config);
            if command == "install" {
                invocation
                    .arg("--root")
                    .arg(directory.path().join("absent"));
            }
            let output = invocation.output().unwrap();
            assert!(!output.status.success());
            assert!(output.stdout.is_empty());
        }
        assert!(!directory.path().join("absent").exists());
    }
}

#[test]
fn offline_install_copies_real_binary_profile_unit_and_enable_link_repeatably() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("root");
    let config = directory.path().join("profile.toml");
    fs::create_dir(&root).unwrap();
    fs::write(&config, "schema_version=1\npacman_keyrings=['pond']").unwrap();
    let invoke = || {
        Command::new(INIT)
            .arg("install")
            .arg("--root")
            .arg(&root)
            .arg("--config")
            .arg(&config)
            .output()
            .unwrap()
    };
    assert!(!invoke().status.success());
    assert!(!root.join("usr").exists());
    oci_kit::staging::mark(&root).unwrap();
    for _ in 0..2 {
        let output = invoke();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let installed = root.join("usr/local/bin/lighos-init");
    assert_eq!(fs::read(&installed).unwrap(), fs::read(INIT).unwrap());
    assert_eq!(
        fs::metadata(&installed).unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert!(
        Command::new(installed)
            .arg("--version")
            .output()
            .unwrap()
            .status
            .success()
    );
    let stored = fs::read_to_string(root.join("etc/oci-kit/lightos.toml")).unwrap();
    lighos_init::Profile::parse(&stored).unwrap();
    let unit = fs::read_to_string(root.join("etc/systemd/system/lighos-init.service")).unwrap();
    assert!(unit.contains("After=lightos-setup.service"));
    assert!(unit.contains("ConditionPathExists=/var/lib/lightos/initialized"));
    assert!(!unit.contains("seed.env"));
    assert_eq!(
        fs::read_link(root.join("etc/systemd/system/multi-user.target.wants/lighos-init.service"))
            .unwrap(),
        std::path::Path::new("../lighos-init.service")
    );
    assert!(!root.join("var/lib/oci-kit/lightos-init.json").exists());
    assert!(!root.join("etc/lightos/seed.env").exists());
}
