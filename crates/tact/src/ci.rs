use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub(crate) enum Task {
    /// Create unique tool manifests before the cache action runs.
    PrepareCache,
    /// Write evidence for upstream post-save to archive.
    SeedCache,
    /// Verify evidence restored from the preceding job.
    VerifyCache,
    /// Check the selected backend and every exact-hit output.
    VerifyHits {
        #[arg(long,action=clap::ArgAction::Set)]
        expected: bool,
    },
}

fn env(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("required CI variable {name}"))
}

pub(crate) fn run(root: &Path, task: &Task) -> Result<()> {
    match task {
        Task::PrepareCache => {
            let identity = format!("{}-{}", env("GITHUB_RUN_ID")?, env("GITHUB_RUN_ATTEMPT")?);
            let directory = root.join(".tars/scratch/ci-cache");
            fs::create_dir_all(&directory)?;
            for file in ["devenv.nix", "devenv.yaml", "devenv.lock"] {
                fs::copy(root.join(file), directory.join(file))?;
            }
            fs::write(
                directory.join("Cargo.toml"),
                format!("[package]\nname = \"cache-fixture\"\nversion = \"0.0.0\"\n# {identity}\n"),
            )?;
            for file in ["uv.lock", "requirements.txt", "bun.lock", "trivy.yaml"] {
                fs::write(directory.join(file), format!("# {identity}\n"))?;
            }
        }
        Task::SeedCache | Task::VerifyCache => {
            let cargo = PathBuf::from(env("CARGO_HOME")?);
            let mut paths = vec![
                cargo.join("registry/index"),
                cargo.join("registry/cache"),
                cargo.join("git/db"),
            ];
            for name in [
                "CARGO_TARGET_DIR",
                "UV_CACHE_DIR",
                "PIP_CACHE_DIR",
                "BUN_INSTALL_CACHE_DIR",
                "TRIVY_CACHE_DIR",
            ] {
                paths.push(PathBuf::from(env(name)?));
            }
            let expected = format!("{}\n", env("GITHUB_RUN_ID")?);
            for path in paths {
                if matches!(task, Task::SeedCache) {
                    fs::create_dir_all(&path)?;
                    fs::write(path.join("tars-cache-proof"), &expected)?;
                } else {
                    let actual = fs::read_to_string(path.join("tars-cache-proof"))?;
                    ensure!(
                        actual == expected,
                        "cache evidence mismatch at {}",
                        path.display()
                    );
                }
            }
        }
        Task::VerifyHits { expected } => {
            ensure!(env("BACKEND")? == "github", "expected GitHub cache backend");
            for name in ["CARGO", "CARGO_TARGET", "UV", "PIP", "BUN", "TRIVY"] {
                ensure!(
                    env(name)? == expected.to_string(),
                    "{name}: expected exact-hit={expected}"
                );
            }
        }
    }
    println!("CI cache fixture check passed.");
    Ok(())
}
