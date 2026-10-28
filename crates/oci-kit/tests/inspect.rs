use std::fs;
use std::io::Write;
use std::path::PathBuf;

use oci_kit::inspect::inspect_archive;

fn archive() -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    let body = b"ID=fixture\n";
    let mut header = tar::Header::new_gnu();
    header.set_size(body.len() as u64);
    header.set_uid(1234);
    header.set_gid(1235);
    header.set_mode(0o640);
    header.set_cksum();
    builder
        .append_data(&mut header, "./etc/os-release", body.as_slice())
        .unwrap();
    builder.into_inner().unwrap()
}

#[test]
fn reads_metadata_and_text_without_extracting() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rootfs.tar");
    fs::write(&path, archive()).unwrap();
    let report = inspect_archive(
        &path,
        &[PathBuf::from("etc")],
        &[PathBuf::from("etc/os-release")],
    )
    .unwrap();
    assert_eq!(report.entries, 1);
    assert_eq!(report.selected[0].uid, 1234);
    assert_eq!(report.selected[0].gid, 1235);
    assert_eq!(report.selected[0].mode, 0o640);
    assert_eq!(
        report.text[&PathBuf::from("etc/os-release")],
        "ID=fixture\n"
    );
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn recognizes_gzip_and_zstd_by_magic_not_extension() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("arbitrary-name");
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(&archive()).unwrap();
    fs::write(&path, gzip.finish().unwrap()).unwrap();
    assert_eq!(inspect_archive(&path, &[], &[]).unwrap().entries, 1);
    fs::write(
        &path,
        zstd::stream::encode_all(archive().as_slice(), 1).unwrap(),
    )
    .unwrap();
    assert_eq!(inspect_archive(&path, &[], &[]).unwrap().entries, 1);
}

#[test]
fn refuses_parent_traversal_and_absolute_archive_paths() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("unsafe.tar");
    for name in [b"../escape".as_slice(), b"/absolute".as_slice()] {
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_mode(0o644);
        header.as_mut_bytes()[..name.len()].copy_from_slice(name);
        header.set_cksum();
        let mut builder = tar::Builder::new(Vec::new());
        builder.append(&header, &[][..]).unwrap();
        fs::write(&path, builder.into_inner().unwrap()).unwrap();
        assert!(inspect_archive(&path, &[], &[]).is_err());
    }
}
