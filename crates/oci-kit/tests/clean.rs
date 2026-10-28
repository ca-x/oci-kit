use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use oci_kit::clean::{Policy, Removal, clean};
use oci_kit::staging::{MARKER, StagingRoot, mark, reject_mounts_in, relative_path};

fn policy(path: &str) -> Policy {
    Policy {
        schema_version: 1,
        remove: vec![Removal {
            path: path.into(),
            reason: "test policy".into(),
        }],
    }
}

#[test]
fn dry_run_preserves_files_and_apply_removes_only_selected_subtree() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir_all(directory.path().join("boot/nested")).unwrap();
    fs::write(directory.path().join("boot/nested/kernel"), b"kernel").unwrap();
    fs::write(directory.path().join("keep"), b"keep").unwrap();
    mark(directory.path()).unwrap();
    let plan = clean(directory.path(), &policy("boot"), false).unwrap();
    assert!(!plan.applied);
    assert!(directory.path().join("boot/nested/kernel").is_file());
    assert!(
        clean(directory.path(), &policy("boot"), true)
            .unwrap()
            .applied
    );
    assert!(!directory.path().join("boot").exists());
    assert!(directory.path().join("keep").is_file());
}

#[test]
fn refuses_unmarked_live_root_and_unsafe_paths() {
    let directory = tempfile::tempdir().unwrap();
    assert!(clean(directory.path(), &policy("boot"), true).is_err());
    assert!(mark(Path::new("/")).is_err());
    for path in ["", ".", "..", "a/../b", "/boot", MARKER] {
        assert!(relative_path(Path::new(path)).is_err(), "accepted {path}");
    }
}

#[test]
fn symlinked_ancestor_cannot_escape_staging() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("precious"), b"keep").unwrap();
    mark(directory.path()).unwrap();
    symlink(outside.path(), directory.path().join("escape")).unwrap();
    assert!(clean(directory.path(), &policy("escape/precious"), true).is_err());
    assert!(outside.path().join("precious").is_file());
    clean(directory.path(), &policy("escape"), true).unwrap();
    assert!(outside.path().join("precious").is_file());
    assert!(fs::symlink_metadata(directory.path().join("escape")).is_err());
}

#[test]
fn mark_rejects_symlinked_ancestor_of_staging_root() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir(outside.path().join("rootfs")).unwrap();
    symlink(outside.path(), directory.path().join("alias")).unwrap();
    assert!(mark(&directory.path().join("alias/rootfs")).is_err());
    assert!(!outside.path().join("rootfs").join(MARKER).exists());
}

#[test]
fn open_rejects_symlinked_ancestor_of_staging_root() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir(outside.path().join("rootfs")).unwrap();
    fs::write(outside.path().join("rootfs/keep"), b"keep").unwrap();
    mark(&outside.path().join("rootfs")).unwrap();
    symlink(outside.path(), directory.path().join("alias")).unwrap();
    assert!(StagingRoot::open(&directory.path().join("alias/rootfs")).is_err());
    assert!(outside.path().join("rootfs/keep").is_file());
}

#[test]
fn preflights_every_rule_before_removing_anything() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("keep"), b"keep").unwrap();
    mark(directory.path()).unwrap();
    let mut rules = policy("keep");
    rules.remove.push(Removal {
        path: "../escape".into(),
        reason: "unsafe".into(),
    });
    assert!(clean(directory.path(), &rules, true).is_err());
    assert!(directory.path().join("keep").is_file());
}

#[test]
fn refuses_symlinked_staging_marker() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(
        outside.path().join("marker"),
        b"oci-kit offline staging v1\n",
    )
    .unwrap();
    symlink(outside.path().join("marker"), directory.path().join(MARKER)).unwrap();
    assert!(StagingRoot::open(directory.path()).is_err());
    assert!(mark(directory.path()).is_err());
}

#[test]
fn rejects_special_marker_by_metadata_before_opening_for_io() {
    use std::os::unix::net::UnixListener;

    let directory = tempfile::tempdir().unwrap();
    let _listener = UnixListener::bind(directory.path().join(MARKER)).unwrap();
    for result in [
        StagingRoot::open(directory.path()).map(|_| ()),
        mark(directory.path()).map(|_| ()),
    ] {
        let error = result.unwrap_err();
        assert!(
            format!("{error:#}").contains("staging marker must be a regular file"),
            "marker was opened for I/O before its type was checked: {error:#}"
        );
    }
}

#[test]
fn refuses_nested_and_escaped_mount_paths() {
    let mounts = "12 1 1:1 / /stage/dev rw - tmpfs tmpfs rw\n13 1 1:2 / /with\\040space/root/mnt rw - tmpfs tmpfs rw\n";
    assert!(reject_mounts_in(Path::new("/stage"), mounts).is_err());
    assert!(reject_mounts_in(Path::new("/with space/root"), mounts).is_err());
    assert!(reject_mounts_in(Path::new("/stage/other"), mounts).is_ok());
}

#[test]
fn staged_writes_refuse_symlink_parents_and_preserve_modes() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    mark(directory.path()).unwrap();
    let root = StagingRoot::open(directory.path()).unwrap();
    root.write_file(Path::new("etc/kit/config"), b"config", 0o600)
        .unwrap();
    assert_eq!(
        fs::metadata(directory.path().join("etc/kit/config"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    symlink("/etc", directory.path().join("escape")).unwrap();
    assert!(
        root.write_file(Path::new("escape/kit-test"), b"no", 0o600)
            .is_err()
    );
}
