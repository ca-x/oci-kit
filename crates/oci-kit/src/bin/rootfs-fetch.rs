use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use oci_kit::fetch::{Source, fetch};

#[derive(Parser)]
#[command(
    version,
    about = "Fetch and verify an explicitly identified rootfs artifact"
)]
struct Args {
    #[arg(long, conflicts_with_all = ["url", "sha256", "size"])]
    source: Option<PathBuf>,
    #[arg(long, required_unless_present = "source", requires = "sha256")]
    url: Option<String>,
    #[arg(long, required_unless_present = "source", requires = "url")]
    sha256: Option<String>,
    #[arg(long)]
    size: Option<u64>,
    #[arg(long)]
    output: PathBuf,
    #[arg(
        long,
        help = "Permit unencrypted HTTP, including redirects; for trusted test networks only"
    )]
    allow_http: bool,
    #[arg(long, default_value_t = 7200, value_parser = clap::value_parser!(u64).range(1..))]
    timeout_seconds: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let source = match args.source {
        Some(path) => serde_json::from_slice(&fs::read(path).context("reading source document")?)?,
        None => Source {
            schema_version: 1,
            url: args.url.context("missing URL")?,
            sha256: args.sha256.context("missing SHA-256")?,
            size: args.size,
            metadata: BTreeMap::new(),
        },
    };
    let result = fetch(
        &source,
        &args.output,
        args.allow_http,
        Duration::from_secs(args.timeout_seconds),
    )?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
