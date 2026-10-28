use std::fs;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand};
use lighos_init::{InitPlan, Profile, apply, generate_systemd, install, plan};
use serde::Serialize;

#[derive(Serialize)]
struct ApplyResult {
    #[serde(flatten)]
    plan: InitPlan,
    #[serde(skip_serializing_if = "Option::is_none")]
    changed: Option<bool>,
}

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
    let program = std::env::args_os().next().unwrap_or_default();
    if Path::new(&program).file_name() == Some(OsStr::new("lighos-init-generator")) {
        let arguments: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
        return generate_systemd(&arguments);
    }
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
            let plan = plan(&profile, should_apply)?;
            let changed = should_apply.then(|| apply(&profile)).transpose()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&ApplyResult { plan, changed })?
            );
        }
    }
    Ok(())
}
