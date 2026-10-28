use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use oci_kit::fetch::{Source, fetch};
use sha2::{Digest, Sha256};

fn source(body: &[u8]) -> Source {
    Source {
        schema_version: 1,
        url: "https://example.invalid/rootfs".into(),
        sha256: format!("{:x}", Sha256::digest(body)),
        size: Some(body.len() as u64),
        metadata: BTreeMap::new(),
    }
}

fn server(body: &'static [u8], content_length: usize) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = [0_u8; 4096];
        let received = stream.read(&mut request).unwrap();
        assert!(received > 0);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {content_length}\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        stream.write_all(body).unwrap();
    });
    (format!("http://{address}/artifact"), worker)
}

#[test]
fn fetches_verified_artifact_and_reuses_cache_without_network() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("rootfs.tar");
    let mut source = source(b"rootfs");
    let (url, worker) = server(b"rootfs", 6);
    source.url = url;
    let first = fetch(&source, &destination, true, Duration::from_secs(5)).unwrap();
    worker.join().unwrap();
    assert!(!first.reused);
    assert_eq!(fs::read(&destination).unwrap(), b"rootfs");
    assert!(directory.path().join("rootfs.tar.source.json").is_file());
    let second = fetch(&source, &destination, true, Duration::from_secs(5)).unwrap();
    assert!(second.reused);
}

#[test]
fn bad_digest_leaves_no_destination_or_temporary_file() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("rootfs.tar");
    let mut source = source(b"correct");
    source.size = None;
    let (url, worker) = server(b"corrupt", 7);
    source.url = url;
    assert!(fetch(&source, &destination, true, Duration::from_secs(5)).is_err());
    worker.join().unwrap();
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn interrupted_download_is_not_persisted() {
    let directory = tempfile::tempdir().unwrap();
    let mut source = source(b"partial");
    source.size = None;
    let (url, worker) = server(b"partial", 100);
    source.url = url;
    assert!(
        fetch(
            &source,
            &directory.path().join("artifact"),
            true,
            Duration::from_secs(5)
        )
        .is_err()
    );
    worker.join().unwrap();
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn invalid_existing_file_is_never_overwritten() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("artifact");
    fs::write(&destination, b"keep me").unwrap();
    assert!(
        fetch(
            &source(b"wanted"),
            &destination,
            false,
            Duration::from_secs(1)
        )
        .is_err()
    );
    assert_eq!(fs::read(destination).unwrap(), b"keep me");
}

#[test]
fn rejects_http_by_default_and_malformed_source() {
    let mut source = source(b"rootfs");
    source.url = "http://example.invalid/artifact".into();
    assert!(source.validate(false).is_err());
    assert!(source.validate(true).is_ok());
    source.sha256 = "not-a-digest".into();
    assert!(source.validate(true).is_err());
    source = self::source(b"rootfs");
    source.schema_version = 2;
    assert!(source.validate(false).is_err());
}

#[test]
fn declared_size_mismatch_is_not_persisted() {
    let directory = tempfile::tempdir().unwrap();
    let mut source = source(b"rootfs");
    source.size = Some(100);
    let (url, worker) = server(b"rootfs", 6);
    source.url = url;
    assert!(
        fetch(
            &source,
            &directory.path().join("artifact"),
            true,
            Duration::from_secs(5)
        )
        .is_err()
    );
    worker.join().unwrap();
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}
