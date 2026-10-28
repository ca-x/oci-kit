use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use lighos_init::{Profile, apply, install, plan};

#[derive(Parser)]
#[command(
    version,
    about = "LightOS-specific post-setup adapter (not a generic rootfs initializer)"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Install {
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        config: PathBuf,
    },
    Apply {
        #[arg(long, default_value = "/etc/oci-kit/lightos.toml")]
        config: PathBuf,
        #[arg(long)]
        apply: bool,
    },
}

fn main() -> Result<()> {
    match Args::parse().command {
        Command::Install { root, config } => {
            let profile = Profile::parse(&fs::read_to_string(config)?)?;
            install(&root, &profile, &std::env::current_exe()?)?;
            println!(
                "installed LightOS post-setup adapter into {}",
                root.display()
            );
        }
        Command::Apply {
            config,
            apply: should_apply,
        } => {
            let profile = Profile::parse(&fs::read_to_string(config)?)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&plan(&profile, should_apply)?)?
            );
            if should_apply {
                println!("changed={}", apply(&profile)?);
            }
        }
    }
    Ok(())
}
