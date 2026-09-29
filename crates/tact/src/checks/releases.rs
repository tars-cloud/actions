use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;

fn load(root: &Path, name: &str) -> Result<Value> {
    Ok(serde_norway::from_str(&fs::read_to_string(
        root.join(format!(".github/workflows/{name}.yaml")),
    )?)?)
}

pub(super) fn contracts(root: &Path) -> Result<()> {
    let prepare = load(root, "repository-release-prepare")?;
    let release = load(root, "repository-release-publish")?;
    for concurrency in [
        &prepare["concurrency"],
        &release["jobs"]["release"]["concurrency"],
    ] {
        ensure!(
            concurrency["group"] == "repository-release"
                && concurrency["cancel-in-progress"] == false
                && concurrency["queue"] == "max",
            "preparation and publication must share a non-cancelling release queue"
        );
    }
    let ci = load(root, "repository-ci")?;
    ensure!(
        ci["concurrency"]["group"]
            .as_str()
            .context("CI group")?
            .contains("github.sha")
            && ci["concurrency"]["cancel-in-progress"]
                == "${{ github.event_name == 'pull_request' }}",
        "later merges must not cancel a release commit's CI"
    );
    let security = load(root, "test-security-workflows")?;
    ensure!(
        security["concurrency"]["group"]
            .as_str()
            .context("security group")?
            .contains("github.run_id")
            && security["concurrency"]["cancel-in-progress"] == false,
        "nested security CI must not cancel another trunk run"
    );
    ensure!(
        release["on"]["workflow_run"]["workflows"] == serde_json::json!(["Repository: CI"])
            && release["on"]["workflow_run"]["branches"] == serde_json::json!(["trunk"])
            && release["on"]["workflow_run"]["types"] == serde_json::json!(["completed"]),
        "release automation must follow trunk CI completions"
    );
    let steps = release["jobs"]["release"]["steps"]
        .as_array()
        .context("release steps")?;
    ensure!(
        steps.first().context("trigger step")?["id"] == "trigger",
        "explain eligibility before setup"
    );
    for step in steps.iter().skip(1) {
        ensure!(
            step["if"].as_str().is_some_and(
                |condition| condition.contains("steps.trigger.outputs.eligible == 'true'")
            ),
            "release setup and mutations require an eligible trigger"
        );
    }
    trigger_scenarios(&steps[0])
}

fn trigger_scenarios(step: &Value) -> Result<()> {
    let script = step["run"].as_str().context("trigger script")?;
    ensure!(
        !script.contains("${{"),
        "event data must enter the trigger script through environment variables"
    );
    for (event, source_event, branch, repo, conclusion, expected) in [
        (
            "workflow_run",
            "push",
            "trunk",
            "owner/actions",
            "success",
            true,
        ),
        (
            "workflow_run",
            "push",
            "trunk",
            "owner/actions",
            "cancelled",
            false,
        ),
        (
            "workflow_run",
            "push",
            "trunk",
            "owner/actions",
            "failure",
            false,
        ),
        (
            "workflow_run",
            "pull_request",
            "trunk",
            "owner/actions",
            "success",
            false,
        ),
        (
            "workflow_run",
            "push",
            "release/next",
            "owner/actions",
            "success",
            false,
        ),
        (
            "workflow_run",
            "push",
            "trunk",
            "fork/actions",
            "success",
            false,
        ),
        ("workflow_dispatch", "", "", "", "", true),
    ] {
        let fixture = tempfile::tempdir()?;
        let output = fixture.path().join("output");
        let summary = fixture.path().join("summary");
        fs::write(&output, "")?;
        let result = Command::new("bash")
            .args(["--noprofile", "--norc", "-euo", "pipefail", "-c", script])
            .env("EVENT", event)
            .env("SOURCE_EVENT", source_event)
            .env("SOURCE_BRANCH", branch)
            .env("SOURCE_REPOSITORY", repo)
            .env("SOURCE_CONCLUSION", conclusion)
            .env("SOURCE_SHA", "a".repeat(40))
            .env("SOURCE_URL", "https://example.invalid/actions/runs/1")
            .env("GITHUB_REPOSITORY", "owner/actions")
            .env("GITHUB_OUTPUT", &output)
            .env("GITHUB_STEP_SUMMARY", &summary)
            .output()?;
        ensure!(
            result.status.success(),
            "trigger script failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        ensure!(
            fs::read_to_string(output)?.contains("eligible=true") == expected,
            "incorrect eligibility: {event}/{source_event}/{branch}/{repo}/{conclusion}"
        );
        let summary = fs::read_to_string(summary)?;
        ensure!(!summary.is_empty(), "every trigger needs an explanation");
        if event == "workflow_run" {
            ensure!(
                summary.contains(conclusion)
                    && summary.contains("https://example.invalid/actions/runs/1"),
                "summary must identify the triggering run and its conclusion"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn release_workflows_protect_publication_and_explain_skips() {
        super::contracts(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .as_path(),
        )
        .unwrap();
    }
}
