mod artifacts;
mod files;
mod state;

use anyhow::{Context, Result, ensure};
use clap::{Subcommand, ValueEnum};
use serde_json::json;

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum PendingReleasePolicy {
    Wait,
    RefreshUnpublished,
}

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
        #[arg(long, value_enum, default_value = "wait")]
        pending_release_policy: PendingReleasePolicy,
        #[arg(long, conflicts_with_all = ["recovery", "replace_unpublished"])]
        recover_pending: bool,
    },
    /// Open or refresh the single release PR from the selected default-branch commit.
    Prepare {
        #[arg(long)]
        commit: String,
        #[arg(long)]
        replace_unpublished: Option<String>,
        #[arg(long, value_enum, default_value = "wait")]
        pending_release_policy: PendingReleasePolicy,
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
        Task::Candidate {
            recover_pending: true,
            ..
        } => repository.recover_pending()?,
        Task::Candidate {
            pending_release_policy,
            ..
        } => repository.candidate(commit, replacement, *pending_release_policy)?,
        Task::Prepare {
            pending_release_policy,
            ..
        } => repository.prepare(commit, replacement, *pending_release_policy)?,
        Task::Publish {
            artifact_run_id,
            manifest_artifact,
            ..
        } => repository.publish(commit, artifact_run_id, manifest_artifact)?,
    };
    if let Some(reason) = result["reason"].as_str() {
        crate::report::note(reason)?;
    }
    if let Some(pending) = result["pending-commit-sha"].as_str() {
        crate::report::note(&format!(
            r#"Pending version: `{}`.

Approved source: `{pending}`.

Dispatch this workflow with `recover-pending: true` to retry the approved source through the current workflow.
"#,
            text(&result, "pending-version")?
        ))?;
        if let Some(url) = result["pending-run-url"]
            .as_str()
            .filter(|url| !url.is_empty())
        {
            crate::report::note(&format!("Original release run: {url}"))?;
        }
    }
    if let Some(path) = std::env::var_os("DEVENV_RESULT_FILE") {
        std::fs::write(path, serde_json::to_vec(&result)?)?;
    }
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}

fn skipped(reason: &str) -> serde_json::Value {
    decision("skipped", reason)
}

fn decision(name: &str, reason: &str) -> serde_json::Value {
    json!({"decision":name, "prepare-ready": "false", "release-ready": "false", "reason": reason})
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
