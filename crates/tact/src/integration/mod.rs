mod cachix;
mod environments;
mod transport;

use anyhow::{Result, ensure};
use clap::Subcommand;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Subcommand)]
pub(crate) enum Integration {
    /// Test pinned RunsOn main/post code against a disposable local S3 endpoint.
    S3,
    /// Test pinned Cachix main/post code with fake Nix and Cachix executables.
    Cachix,
    /// Test declared and missing Trivy in actual Nix environments.
    Environments {
        #[arg(long)]
        direct: bool,
    },
}

pub(crate) fn run(root: &Path, suite: &Integration) -> Result<()> {
    let scratch = root.join(".tars/scratch/integration");
    fs::create_dir_all(&scratch)?;
    let fixture = tempfile::tempdir_in(scratch)?;
    match suite {
        Integration::S3 => transport::run(fixture.path())?,
        Integration::Cachix => cachix::run(root, fixture.path())?,
        Integration::Environments { direct } => environments::run(root, fixture.path(), *direct)?,
    }
    Ok(())
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

pub(crate) use cachix::mock;
