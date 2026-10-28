use std::fs;
use std::process::Command;

const INIT: &str = env!("CARGO_BIN_EXE_lighos-init");

#[test]
fn offline_install_orders_platform_desktop_units_without_activating_or_replacing_them() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("root");
    fs::create_dir(&root).unwrap();
    oci_kit::staging::mark(&root).unwrap();
    let units = [
        "lightos-remote-desktop.service",
        "lightos-remote-desktop-session.service",
    ];
    fs::create_dir_all(root.join("usr/lib/systemd/system")).unwrap();
    fs::create_dir_all(root.join("etc/lightos")).unwrap();
    for name in units {
        fs::write(
            root.join("usr/lib/systemd/system").join(name),
            "opaque package unit\n",
        )
        .unwrap();
    }
    let environment = root.join("etc/lightos/remote-desktop.env");
    fs::write(&environment, "opaque platform-owned fixture\n").unwrap();
    let config = directory.path().join("profile.toml");
    fs::write(
        &config,
        "schema_version=1\npost_setup_services=['lightos-remote-desktop.service','lightos-remote-desktop-session.service']\n",
    )
    .unwrap();
    for _attempt in 0..2 {
        let output = Command::new(INIT)
            .args(["install", "--root"])
            .arg(&root)
            .arg("--config")
            .arg(&config)
            .env("PATH", "/no-commands-during-offline-install")
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
        for name in units {
            let drop_in = root.join(format!(
                "etc/systemd/system/{name}.d/20-oci-kit-post-setup.conf"
            ));
            assert_eq!(
                fs::read_to_string(drop_in).unwrap(),
                "[Unit]\nRequires=lighos-init.service\nAfter=lighos-init.service\n"
            );
            assert_eq!(
                fs::read_to_string(root.join("usr/lib/systemd/system").join(name)).unwrap(),
                "opaque package unit\n"
            );
            assert!(!root.join(format!("etc/systemd/system/{name}")).exists());
            assert!(
                !root
                    .join(format!("etc/systemd/system/multi-user.target.wants/{name}"))
                    .exists()
            );
        }
        let unit = fs::read_to_string(root.join("etc/systemd/system/lighos-init.service")).unwrap();
        assert!(unit.contains("AssertPathExists=/var/lib/lightos/initialized"));
        assert!(!unit.contains("ConditionPathExists=/var/lib/lightos/initialized"));
        assert_eq!(
            fs::read_to_string(&environment).unwrap(),
            "opaque platform-owned fixture\n"
        );
        assert!(!root.join("var/lib/oci-kit").exists());
    }
}

#[test]
fn unsafe_recursive_and_duplicate_platform_unit_names_fail_before_installation() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("profile.toml");
    for names in [
        "['../escape.service']",
        "['/absolute.service']",
        "['--help.service']",
        "['unit.service;whoami']",
        "['multi-user.target']",
        "['lighos-init.service']",
        "['lightos-setup.service']",
        "['lightos-.service']",
        "['lighos-.service']",
        "['desktop-.service']",
        "['desktop@.service']",
        "['desktop@@session.service']",
        "['desktop.service','desktop.service']",
    ] {
        fs::write(
            &config,
            format!("schema_version=1\npost_setup_services={names}\n"),
        )
        .unwrap();
        let output = Command::new(INIT)
            .args(["install", "--root"])
            .arg(directory.path().join("absent"))
            .arg("--config")
            .arg(&config)
            .output()
            .unwrap();
        assert!(!output.status.success(), "accepted {names}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("platform service"));
    }
}

#[test]
fn reinstall_reconciles_only_owned_platform_drop_ins() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("root");
    fs::create_dir(&root).unwrap();
    oci_kit::staging::mark(&root).unwrap();
    let config = directory.path().join("profile.toml");
    let first = root.join("etc/systemd/system/first.service.d/20-oci-kit-post-setup.conf");
    let second = root.join("etc/systemd/system/second.service.d/20-oci-kit-post-setup.conf");
    for names in ["['first.service']", "['second.service']", "[]"] {
        fs::write(
            &config,
            format!("schema_version=1\npost_setup_services={names}\n"),
        )
        .unwrap();
        let output = Command::new(INIT)
            .args(["install", "--root"])
            .arg(&root)
            .arg("--config")
            .arg(&config)
            .env("PATH", "/no-commands-during-offline-install")
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(first.exists(), names == "['first.service']");
        assert_eq!(second.exists(), names == "['second.service']");
        let unmanaged = first.parent().unwrap().join("90-platform.conf");
        if names == "['first.service']" {
            fs::write(&unmanaged, "platform owns this\n").unwrap();
        }
        assert_eq!(fs::read_to_string(unmanaged).unwrap(), "platform owns this\n");
    }
}

#[test]
fn reinstall_refuses_modified_or_symlinked_stale_drop_ins_before_replacing_the_profile() {
    for condition in ["modified", "symlink", "directory"] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("root");
        fs::create_dir(&root).unwrap();
        oci_kit::staging::mark(&root).unwrap();
        let config = directory.path().join("profile.toml");
        fs::write(
            &config,
            "schema_version=1\npost_setup_services=['first.service']\n",
        )
        .unwrap();
        let install = || {
            Command::new(INIT)
                .args(["install", "--root"])
                .arg(&root)
                .arg("--config")
                .arg(&config)
                .output()
                .unwrap()
        };
        assert!(install().status.success());
        let installed = root.join("etc/oci-kit/lightos.toml");
        let previous = fs::read(&installed).unwrap();
        let drop_in = root.join("etc/systemd/system/first.service.d/20-oci-kit-post-setup.conf");
        fs::remove_file(&drop_in).unwrap();
        let outside = directory.path().join("outside");
        fs::write(&outside, "do not modify\n").unwrap();
        match condition {
            "modified" => fs::write(&drop_in, "unrecognized contents\n").unwrap(),
            "symlink" => std::os::unix::fs::symlink(&outside, &drop_in).unwrap(),
            _ => fs::create_dir(&drop_in).unwrap(),
        }
        fs::write(&config, "schema_version=1\npost_setup_services=[]\n").unwrap();
        let output = install();
        assert!(!output.status.success(), "accepted {condition} stale drop-in");
        assert!(output.stdout.is_empty());
        assert_eq!(fs::read(&installed).unwrap(), previous);
        assert!(fs::symlink_metadata(&drop_in).is_ok());
        assert_eq!(fs::read_to_string(&outside).unwrap(), "do not modify\n");
    }
}
