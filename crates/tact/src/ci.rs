use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub(crate) enum Task {
    /// Generate remote action references pinned to the exact CI revision.
    PrepareConsumer,
    /// Create unique tool manifests before the cache action runs.
    PrepareCache {
        /// Clear only this run's dedicated S3 fixture archives before restoration.
        #[arg(long)]
        reset_s3_fixture: bool,
    },
    /// Write evidence for upstream post-save to archive.
    SeedCache,
    /// Verify evidence restored from the preceding job.
    VerifyCache,
    /// Check the selected backend and every exact-hit output.
    VerifyHits {
        #[arg(long,action=clap::ArgAction::Set)]
        expected: bool,
        #[arg(long, default_value = "github", value_parser = ["github", "s3"])]
        backend: String,
    },
}

fn env(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("required CI variable {name}"))
}

pub(crate) fn run(root: &Path, task: &Task) -> Result<()> {
    match task {
        Task::PrepareConsumer => {
            // Workflow uses references cannot contain expressions; resolve the SHA before loading a local composite.
            let action = consumer_action(&env("GITHUB_REPOSITORY")?, &env("GITHUB_SHA")?)?;
            let directory =
                PathBuf::from(env("GITHUB_WORKSPACE")?).join(".tars/scratch/consumer-action");
            fs::create_dir_all(&directory)?;
            fs::write(
                directory.join("action.yml"),
                format!(
                    "---\n{}",
                    serde_norway::to_string(&action)?.replace("\n  - id:", "\n\n  - id:")
                ),
            )?;
        }
        Task::PrepareCache { reset_s3_fixture } => {
            let identity = format!("{}-{}", env("GITHUB_RUN_ID")?, env("GITHUB_RUN_ATTEMPT")?);
            if *reset_s3_fixture {
                ensure!(
                    identity.chars().all(|c| c.is_ascii_digit() || c == '-'),
                    "invalid run identity"
                );
                let cache = PathBuf::from(env("RUNNER_TEMP")?).join(format!("tact-s3-{identity}"));
                if cache.exists() {
                    fs::remove_dir_all(cache)?;
                }
            }
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
        Task::VerifyHits { expected, backend } => {
            ensure!(
                env("BACKEND")? == *backend,
                "expected {backend} cache backend"
            );
            for name in ["CARGO", "CARGO_TARGET", "UV", "PIP", "BUN", "TRIVY"] {
                ensure!(
                    env(name)? == expected.to_string(),
                    "{name}: expected exact-hit={expected}"
                );
                let status = env(&format!("{name}_STATUS"))?;
                ensure!(
                    if *expected {
                        status == "hit"
                    } else {
                        matches!(status.as_str(), "fallback" | "miss-or-unavailable")
                    },
                    "{name}: unexpected restore status {status}"
                );
            }
        }
    }
    println!("CI cache fixture check passed.");
    Ok(())
}

fn consumer_action(repository: &str, revision: &str) -> Result<serde_json::Value> {
    use serde_json::json;
    ensure!(
        repository.split('/').count() == 2
            && repository.split('/').all(|part| !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)))
            && revision.len() == 40
            && revision.chars().all(|c| c.is_ascii_hexdigit()),
        "consumer fixture requires owner/repository and a full commit SHA"
    );
    let reference = |action| format!("{repository}/composite/{action}@{revision}");
    let environment = json!({
        "type": "flakes",
        "working-directory": "consumer-checkout/tests/fixtures/flakes",
        "flake-shell": ".#named"
    });
    let mut cache = environment.clone();
    cache["tools"] = json!("trivy");
    cache["trivy-cache-path"] = json!(".cache/trivy");
    let mut execution = environment.clone();
    execution["run"] = json!(
        "test \"$FIXTURE_SHELL\" = named; devenv-flake-test; printf '{\"nested\":true}' > \"$DEVENV_RESULT_FILE\""
    );
    Ok(json!({
        "name": "Remote consumer fixture",
        "description": "Exercise action-owned scripts independently of a nested consumer checkout.",
        "outputs": {
            "result": {"description": "Nested consumer command result", "value": "${{ steps.run.outputs.result }}"},
            "backend": {"description": "Selected cache backend", "value": "${{ steps.cache.outputs.backend }}"},
            "tools": {"description": "Selected cache tools", "value": "${{ steps.cache.outputs.tools }}"},
            "cachix-mode": {"description": "Selected Cachix mode", "value": "${{ steps.nix-cache.outputs.cachix-mode }}"}
        },
        "runs": {"using": "composite", "steps": [
            {"id": "cache", "name": "Restore consumer cache", "uses": reference("setup-cache"), "with": cache},
            {"id": "nix-cache", "name": "Configure consumer Nix cache", "uses": reference("setup-nix-cache"), "with": {"cachix-name": "devenv"}},
            {"id": "devenv", "name": "Prepare consumer shell", "uses": reference("setup-devenv"), "with": environment},
            {"id": "trivy", "name": "Validate consumer Trivy", "uses": reference("setup-trivy"), "with": environment},
            {"id": "run", "name": "Run consumer shell commands", "uses": reference("run-devenv"), "with": execution}
        ]}
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_consumer_requires_immutable_references() -> Result<()> {
        for revision in ["trunk", "${{ github.sha }}", "abcd"] {
            assert!(consumer_action("example/actions", revision).is_err());
        }
        let revision = "a".repeat(40);
        assert!(consumer_action("../example/actions", &revision).is_err());
        let action = consumer_action("example/actions", &revision)?;
        for step in action["runs"]["steps"].as_array().unwrap() {
            let reference = step["uses"].as_str().unwrap();
            assert!(reference.starts_with("example/actions/composite/"));
            assert!(reference.ends_with(&format!("@{revision}")));
            if step["id"] == "nix-cache" {
                assert!(step["with"]["working-directory"].is_null());
                assert_eq!(step["with"]["cachix-name"], "devenv");
            } else {
                assert_eq!(
                    step["with"]["working-directory"],
                    "consumer-checkout/tests/fixtures/flakes"
                );
            }
        }
        Ok(())
    }
}
