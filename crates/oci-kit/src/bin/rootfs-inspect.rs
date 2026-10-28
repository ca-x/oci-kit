use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use oci_kit::inspect::inspect_archive;

#[derive(Parser)]
#[command(
    version,
    about = "Inspect tar, tar.gz or tar.zst metadata without extracting it"
)]
struct Args {
    #[arg(long)]
    archive: PathBuf,
    #[arg(
        long,
        help = "Include metadata for this exact path or subtree; repeatable"
    )]
    select: Vec<PathBuf>,
    #[arg(long, help = "Read a regular UTF-8 file of at most 64 KiB; repeatable")]
    text: Vec<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let report = inspect_archive(&args.archive, &args.select, &args.text)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
