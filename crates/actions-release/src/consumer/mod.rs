mod artifacts;
mod files;
mod state;

use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use serde_json::json;

#[derive(Subcommand)]
pub(crate) enum Task {
    /// Identify an ordinary merge, a merged release PR, or completed work.
    Candidate {
        #[arg(long)]
        commit: String,
        #[arg(long)]
        recovery: bool,
        #[arg(long, conflicts_with = "recovery")]
        replace_unpublished: Option<String>,
    },
    /// Open or refresh the single release PR from the selected default-branch commit.
    Prepare {
        #[arg(long)]
        commit: String,
        #[arg(long)]
        replace_unpublished: Option<String>,
    },
    /// Attach the declared artifacts and publish the approved version.
    Publish {
        #[arg(long)]
        commit: String,
        #[arg(long)]
        artifact_run_id: String,
        #[arg(long)]
        manifest_artifact: String,
        #[arg(long)]
        recovery: bool,
    },
}

pub(crate) fn execute(task: &Task) -> Result<()> {
    let (Task::Candidate { commit, .. }
    | Task::Prepare { commit, .. }
    | Task::Publish { commit, .. }) = task;
    crate::project::full_sha(commit)?;
    let replacement = match task {
        Task::Candidate {
            replace_unpublished,
            ..
        }
        | Task::Prepare {
            replace_unpublished,
            ..
        } => replace_unpublished.as_deref(),
        Task::Publish { .. } => None,
    };
    if let Some(replaced) = replacement {
        crate::project::full_sha(replaced)?;
    }
    let recovery = matches!(
        task,
        Task::Candidate { recovery: true, .. } | Task::Publish { recovery: true, .. }
    );
    let repository = state::Repository::open(commit, recovery)?;
    let result = match task {
        Task::Candidate { .. } => repository.candidate(commit, replacement)?,
        Task::Prepare { .. } => repository.prepare(commit, replacement)?,
        Task::Publish {
            artifact_run_id,
            manifest_artifact,
            ..
        } => repository.publish(commit, artifact_run_id, manifest_artifact)?,
    };
    if let Some(path) = std::env::var_os("DEVENV_RESULT_FILE") {
        std::fs::write(path, serde_json::to_vec(&result)?)?;
    }
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}

fn skipped(reason: &str) -> serde_json::Value {
    json!({"prepare-ready": "false", "release-ready": "false", "reason": reason})
}

fn numeric(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) && value != "0",
        "expected a positive numeric GitHub identifier"
    );
    Ok(())
}

fn text<'a>(value: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .with_context(|| format!("missing GitHub {key}"))
}

fn git(args: &[&str]) -> Result<String> {
    crate::run(
        std::process::Command::new("git")
            .args([
                "-c",
                "credential.helper=",
                "-c",
                "credential.helper=!gh auth git-credential",
            ])
            .args(args),
    )
}
