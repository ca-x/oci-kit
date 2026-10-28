use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const FETCH: &str = env!("CARGO_BIN_EXE_rootfs-fetch");
const INSPECT: &str = env!("CARGO_BIN_EXE_rootfs-inspect");
const CLEANER: &str = env!("CARGO_BIN_EXE_rootfs-cleaner");

fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn failure(output: Output, message: &str) {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(message),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn server(response: Vec<u8>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "CLI never contacted fixture");
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("{error}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = [0_u8; 4096];
        assert!(stream.read(&mut request).unwrap() > 0);
        stream.write_all(&response).unwrap();
    });
    (format!("http://{address}/rootfs"), worker)
}

fn source(url: &str, body: &[u8]) -> Value {
    json!({
        "schema_version": 1,
        "url": url,
        "sha256": format!("{:x}", Sha256::digest(body)),
        "size": body.len(),
        "metadata": {"origin_commit": "fixture"}
    })
}

fn archive(body: &[u8]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(body.len() as u64);
    header.set_mode(0o640);
    header.set_uid(1234);
    header.set_gid(1235);
    header.set_cksum();
    builder
        .append_data(&mut header, "etc/os-release", body)
        .unwrap();
    builder.into_inner().unwrap()
}

#[test]
fn every_generic_binary_has_help_and_version() {
    for binary in [FETCH, INSPECT, CLEANER] {
        for flag in ["--help", "--version"] {
            let output = Command::new(binary).arg(flag).output().unwrap();
            assert!(output.status.success());
            assert!(!output.stdout.is_empty());
            assert!(output.stderr.is_empty());
        }
    }
}

#[test]
fn fetch_downloads_with_provenance_and_reverifies_cache_without_network() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("rootfs");
    let source_path = directory.path().join("source.json");
    let (url, worker) =
        server(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nrootfs".to_vec());
    let document = source(&url, b"rootfs");
    fs::write(&source_path, serde_json::to_vec(&document).unwrap()).unwrap();
    let invoke = || {
        Command::new(FETCH)
            .arg("--source")
            .arg(&source_path)
            .arg("--output")
            .arg(&output)
            .arg("--allow-http")
            .args(["--timeout-seconds", "5"])
            .output()
            .unwrap()
    };
    let first = success(invoke());
    worker.join().unwrap();
    assert_eq!(first["reused"], false);
    assert_eq!(first["bytes"], 6);
    assert_eq!(fs::read(&output).unwrap(), b"rootfs");
    let sidecar: Value =
        serde_json::from_slice(&fs::read(directory.path().join("rootfs.source.json")).unwrap())
            .unwrap();
    assert_eq!(sidecar, document);
    assert_eq!(success(invoke())["reused"], true);
    fs::write(&output, b"keep me").unwrap();
    failure(invoke(), "refusing to overwrite");
    assert_eq!(fs::read(output).unwrap(), b"keep me");
}

#[test]
fn fetch_failed_transfers_leave_no_artifact_or_temporary_files() {
    for (response, expected, declared_size, message) in [
        (
            "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nbad",
            b"yes".as_slice(),
            3,
            "SHA-256 mismatch",
        ),
        (
            "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nyes",
            b"yes".as_slice(),
            4,
            "Content-Length",
        ),
        (
            "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nyes",
            b"yes".as_slice(),
            100,
            "reading artifact data",
        ),
        (
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            b"yes".as_slice(),
            3,
            "404",
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("download/rootfs");
        let (url, worker) = server(response.as_bytes().to_vec());
        let output = Command::new(FETCH)
            .args([
                "--url",
                &url,
                "--sha256",
                &format!("{:x}", Sha256::digest(expected)),
                "--size",
                &declared_size.to_string(),
                "--allow-http",
                "--timeout-seconds",
                "5",
            ])
            .arg("--output")
            .arg(&destination)
            .output()
            .unwrap();
        worker.join().unwrap();
        failure(output, message);
        assert_eq!(
            fs::read_dir(destination.parent().unwrap()).unwrap().count(),
            0
        );
    }
}

#[test]
fn fetch_rejects_insecure_and_invalid_sources_before_touching_output() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.json");
    let destination = directory.path().join("untouched/rootfs");
    for (url, message) in [
        ("http://example.invalid/rootfs", "only HTTPS"),
        ("file:///tmp/rootfs", "only HTTPS"),
        ("https://user:secret@example.invalid/rootfs", "credentials"),
        ("https://example.invalid/rootfs#part", "fragments"),
    ] {
        fs::write(&path, serde_json::to_vec(&source(url, b"rootfs")).unwrap()).unwrap();
        failure(
            Command::new(FETCH)
                .arg("--source")
                .arg(&path)
                .arg("--output")
                .arg(&destination)
                .output()
                .unwrap(),
            message,
        );
        assert!(!destination.parent().unwrap().exists());
    }
    let mut document = source("https://example.invalid/rootfs", b"rootfs");
    document["unexpected"] = json!(true);
    fs::write(&path, document.to_string()).unwrap();
    failure(
        Command::new(FETCH)
            .arg("--source")
            .arg(&path)
            .arg("--output")
            .arg(&destination)
            .output()
            .unwrap(),
        "unknown field",
    );
    failure(
        Command::new(FETCH)
            .args(["--url", "https://example.invalid/rootfs"])
            .arg("--output")
            .arg(destination)
            .output()
            .unwrap(),
        "--sha256",
    );
}

#[test]
fn inspect_streams_all_formats_and_reports_metadata_as_one_json_document() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("no-extension");
    let tar = archive(b"ID=fixture\n");
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(&tar).unwrap();
    for data in [
        tar.clone(),
        gzip.finish().unwrap(),
        zstd::stream::encode_all(tar.as_slice(), 1).unwrap(),
    ] {
        fs::write(&path, data).unwrap();
        let report = success(
            Command::new(INSPECT)
                .arg("--archive")
                .arg(&path)
                .args(["--select", "etc", "--text", "etc/os-release"])
                .output()
                .unwrap(),
        );
        assert_eq!(report["entries"], 1);
        assert_eq!(report["selected"][0]["uid"], 1234);
        assert_eq!(report["selected"][0]["mode"], 0o640);
        assert_eq!(report["text"]["etc/os-release"], "ID=fixture\n");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}

#[test]
fn inspect_text_failures_have_nonzero_exit_and_no_success_report() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rootfs.tar");
    for (body, text, message) in [
        (
            b"ID=fixture".to_vec(),
            "etc/missing",
            "requested text not found",
        ),
        (vec![0xff], "etc/os-release", "not valid UTF-8"),
        (vec![b'a'; 65537], "etc/os-release", "exceeds 64 KiB"),
        (Vec::new(), "../escape", "unsafe archive entry path"),
    ] {
        fs::write(&path, archive(&body)).unwrap();
        failure(
            Command::new(INSPECT)
                .arg("--archive")
                .arg(&path)
                .args(["--text", text])
                .output()
                .unwrap(),
            message,
        );
    }
}

fn clean(root: &Path, policy: &Path, apply: bool) -> Output {
    let mut command = Command::new(CLEANER);
    command
        .arg("clean")
        .arg("--root")
        .arg(root)
        .arg("--policy")
        .arg(policy);
    if apply {
        command.arg("--apply");
    }
    command.output().unwrap()
}

#[test]
fn cleaner_requires_mark_then_plans_applies_and_repeats_without_following_links() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("root");
    fs::create_dir_all(root.join("cache/subdir")).unwrap();
    fs::write(root.join("cache/subdir/data"), b"remove").unwrap();
    fs::write(directory.path().join("outside"), b"keep").unwrap();
    std::os::unix::fs::symlink(directory.path().join("outside"), root.join("link")).unwrap();
    let policy = directory.path().join("clean.toml");
    fs::write(&policy, "schema_version=1\n[[remove]]\npath='cache'\nreason='fixture'\n[[remove]]\npath='link'\nreason='link only'\n").unwrap();
    failure(clean(&root, &policy, true), "not a marked offline rootfs");
    let mark = Command::new(CLEANER)
        .arg("mark")
        .arg("--root")
        .arg(&root)
        .output()
        .unwrap();
    assert!(
        mark.status.success(),
        "{}",
        String::from_utf8_lossy(&mark.stderr)
    );
    assert_eq!(success(clean(&root, &policy, false))["applied"], false);
    assert!(root.join("cache/subdir/data").exists());
    assert_eq!(success(clean(&root, &policy, true))["applied"], true);
    assert!(!root.join("cache").exists());
    assert!(fs::symlink_metadata(root.join("link")).is_err());
    assert_eq!(fs::read(directory.path().join("outside")).unwrap(), b"keep");
    assert!(success(clean(&root, &policy, true))["remove"][0]["kind"].is_null());
}

#[test]
fn cleaner_validates_all_rules_before_deleting_anything() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("keep"), b"keep").unwrap();
    assert!(
        Command::new(CLEANER)
            .arg("mark")
            .arg("--root")
            .arg(&root)
            .output()
            .unwrap()
            .status
            .success()
    );
    let policy = directory.path().join("clean.toml");
    fs::write(&policy, "schema_version=1\n[[remove]]\npath='keep'\nreason='first'\n[[remove]]\npath='../escape'\nreason='invalid'\n").unwrap();
    failure(clean(&root, &policy, true), "relative");
    assert_eq!(fs::read(root.join("keep")).unwrap(), b"keep");
    failure(
        Command::new(CLEANER)
            .args(["mark", "--root", "/"])
            .output()
            .unwrap(),
        "root",
    );
}
