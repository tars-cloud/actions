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

pub(super) type Execute<'a> =
    dyn FnMut(&mut Command, &Path, u64) -> Result<crate::process::ResultOutput> + 'a;

#[cfg(test)]
fn command_env(command: &Command, name: &str) -> String {
    command
        .get_envs()
        .find(|(key, _)| *key == name)
        .and_then(|(_, value)| value)
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_integration_reports_missing_fixture_instead_of_succeeding() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let error = run(
            directory.path(),
            &Integration::CargoCrap {
                backend: "llvm-cov".into(),
                environment: "devenv".into(),
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("No such file"));
        Ok(())
    }
}
