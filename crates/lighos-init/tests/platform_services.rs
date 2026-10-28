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
        fs::write(root.join("usr/lib/systemd/system").join(name), "opaque package unit\n")
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
        let unit = fs::read_to_string(root.join("etc/systemd/system/lighos-init.service"))
            .unwrap();
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
    }
}
