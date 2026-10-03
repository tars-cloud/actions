mod environments;
mod results;
mod transport;

use anyhow::{Result, ensure};
use clap::Subcommand;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Subcommand)]
pub(crate) enum Integration {
    /// Exercise real Rust coverage and comparison through a selected consumer shell.
    CargoCrap {
        #[arg(long, default_value = "llvm-cov", value_parser = ["llvm-cov", "tarpaulin"])]
        backend: String,
        #[arg(long, default_value = "devenv", value_parser = ["devenv", "flakes"])]
        environment: String,
    },
    /// Test result files through real direct and flake shell execution.
    Results,
    /// Test pinned RunsOn main/post code against a disposable local S3 endpoint.
    S3,
    /// Test declared and missing Trivy in actual Nix environments.
    Environments {
        #[arg(long)]
        direct: bool,
        /// Exercise shell selection and execution for a native or preconfigured emulated system.
        #[arg(long, value_parser = ["x86_64-linux", "aarch64-linux"])]
        system: Option<String>,
    },
}

pub(crate) fn run(root: &Path, suite: &Integration) -> Result<()> {
    let scratch = root.join(".tars/scratch/integration");
    fs::create_dir_all(&scratch)?;
    let fixture = tempfile::tempdir_in(scratch)?;
    match suite {
        Integration::CargoCrap {
            backend,
            environment,
        } => crate::checks::cargo_crap::native(root, backend, environment),
        Integration::Results => results::run(root, fixture.path()),
        Integration::S3 => transport::run(root, fixture.path()),
        Integration::Environments { direct, system } => {
            if let Some(system) = system {
                environments::system(root, fixture.path(), system)
            } else {
                environments::run(root, fixture.path(), *direct)
            }
        }
    }
}

fn download(root: &Path, url: &str, name: &str) -> Result<PathBuf> {
    let file = root.join(name);
    let result = crate::process::run(
        Command::new("curl")
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                url,
                "--output",
            ])
            .arg(&file),
        root,
        60,
    )?;
    ensure!(
        result.code == Some(0),
        "fetch pinned upstream source: {}",
        result.text
    );
    Ok(file)
}
