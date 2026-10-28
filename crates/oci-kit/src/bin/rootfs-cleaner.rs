use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use oci_kit::{clean, staging};

#[derive(Parser)]
#[command(
    version,
    about = "Plan or apply explicit deletions in a marked offline rootfs"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Explicitly designate an unmounted directory as offline staging")]
    Mark {
        #[arg(long)]
        root: PathBuf,
    },
    #[command(about = "Print a deletion plan; do not delete unless --apply is supplied")]
    Clean {
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        policy: PathBuf,
        #[arg(long)]
        apply: bool,
    },
}

fn main() -> Result<()> {
    match Args::parse().command {
        Command::Mark { root } => {
            println!("{}", staging::mark(&root)?.display());
        }
        Command::Clean {
            root,
            policy,
            apply,
        } => {
            let policy =
                toml::from_str(&fs::read_to_string(policy).context("reading cleanup policy")?)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&clean::clean(&root, &policy, apply)?)?
            );
        }
    }
    Ok(())
}
