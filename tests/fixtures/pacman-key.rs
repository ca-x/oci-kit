use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

fn main() -> ExitCode {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(arguments.len(), 2);
    assert!(matches!(arguments[0].as_str(), "--add" | "--populate"));
    writeln!(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open("/calls")
            .unwrap(),
        "{}",
        arguments.join(" ")
    )
    .unwrap();
    fs::write("/observed-path", std::env::var("PATH").unwrap()).unwrap();
    println!("fixture pacman-key output");
    eprintln!("fixture pacman-key diagnostics");
    if arguments[0] == "--add" && fs::read(&arguments[1]).unwrap() == b"invalid" {
        return ExitCode::FAILURE;
    }
    if Path::new("/fail").exists() {
        return ExitCode::FAILURE;
    }
    if Path::new("/block").exists() {
        fs::write("/entered", b"ready").unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !Path::new("/release").exists() {
            assert!(Instant::now() < deadline, "fixture release timed out");
            thread::sleep(Duration::from_millis(20));
        }
    }
    ExitCode::SUCCESS
}
