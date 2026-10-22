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
    let candidate = load(root, "consumer-rust-release-candidate")?;
    ensure!(
        candidate["on"]["workflow_call"]["inputs"]["job-name"]["default"]
            == "Identify Rust Release Candidate"
            && candidate["jobs"]["release"]["name"] == "${{ inputs.job-name }}"
            && ci["jobs"]["rust-release-candidate"]["with"]["job-name"]
                == "Identify Rust Release Candidate - ${{ matrix.architecture }}",
        "label release candidate variants by architecture"
    );
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
    consumer_recovery_contracts(root)?;
    consumer_replacement_contracts(root)?;
    trigger_scenarios(&steps[0])
}

fn consumer_replacement_contracts(root: &Path) -> Result<()> {
    let action: Value = serde_norway::from_str(&fs::read_to_string(
        root.join("composite/release-rust/action.yaml"),
    )?)?;
    let mut scripts = Vec::new();
    for operation in ["candidate", "prepare"] {
        let workflow = load(root, &format!("consumer-rust-release-{operation}"))?;
        ensure!(
            workflow["on"]["workflow_call"]["inputs"]["replace-unpublished"]["default"] == "",
            "candidate replacement must be opt-in"
        );
        let steps = workflow["jobs"]["release"]["steps"]
            .as_array()
            .context("release steps")?;
        let release = steps
            .iter()
            .find(|step| step["id"] == "release")
            .context("release operation")?;
        ensure!(
            release["with"]["replace-unpublished"] == "${{ inputs.replace-unpublished }}",
            "forward candidate replacement to the bundled action"
        );
        scripts.push((
            operation,
            steps[0]["run"]
                .as_str()
                .context("trigger script")?
                .to_owned(),
        ));
    }
    ensure!(
        action["inputs"]["replace-unpublished"]["default"] == "",
        "bundled replacement must be opt-in"
    );
    let steps = action["runs"]["steps"].as_array().context("action steps")?;
    for id in ["trigger", "release"] {
        let step = steps
            .iter()
            .find(|step| step["id"] == id)
            .context("action operation")?;
        ensure!(
            step["env"]["RELEASE_REPLACE_UNPUBLISHED"] == "${{ inputs.replace-unpublished }}",
            "forward replacement input through the action environment"
        );
    }
    let trigger = steps[0]["run"].as_str().context("action trigger")?;
    for operation in ["candidate", "prepare", "publish"] {
        scripts.push((operation, trigger.to_owned()));
    }
    for (operation, script) in scripts {
        for (event, replaced, recovery, expected) in [
            ("push", String::new(), "false", true),
            (
                "workflow_dispatch",
                "b".repeat(40),
                "false",
                operation != "publish",
            ),
            ("push", "b".repeat(40), "false", false),
            ("workflow_dispatch", "pending".to_owned(), "false", false),
            ("workflow_dispatch", "b".repeat(40), "true", false),
        ] {
            let output = Command::new("bash")
                .args(["-euo", "pipefail", "-c", &script])
                .env("ACTION_ROOT", root.join("composite/release-rust"))
                .env("RELEASE_COMMIT", "a".repeat(40))
                .env("GITHUB_SHA", "a".repeat(40))
                .env("GITHUB_REF", "refs/heads/trunk")
                .env("DEFAULT_BRANCH", "trunk")
                .env("GITHUB_EVENT_NAME", event)
                .env("RELEASE_RECOVERY", recovery)
                .env("RELEASE_OPERATION", operation)
                .env("RELEASE_REPLACE_UNPUBLISHED", replaced)
                .output()?;
            ensure!(
                output.status.success() == expected,
                "incorrect {operation} replacement trigger for {event}/{recovery}"
            );
        }
    }
    Ok(())
}

fn consumer_recovery_contracts(root: &Path) -> Result<()> {
    for operation in ["candidate", "publish"] {
        let workflow = load(root, &format!("consumer-rust-release-{operation}"))?;
        ensure!(
            workflow["on"]["workflow_call"]["inputs"]["recovery"]["default"] == false,
            "recovery must be opt-in"
        );
        let steps = workflow["jobs"]["release"]["steps"]
            .as_array()
            .context("release steps")?;
        let operation_step = steps
            .iter()
            .find(|step| step["id"] == "release")
            .context("release operation")?;
        ensure!(
            operation_step["with"]["recovery"] == "${{ inputs.recovery }}",
            "the shared workflow must forward recovery to its bundled action"
        );
        let script = steps[0]["run"].as_str().context("release trigger script")?;
        for (event, reference, recovery, same_commit, expected) in [
            ("push", "refs/heads/trunk", "false", true, true),
            ("push", "refs/heads/trunk", "false", false, false),
            ("push", "refs/heads/trunk", "true", false, false),
            ("workflow_dispatch", "refs/heads/trunk", "true", false, true),
            (
                "workflow_dispatch",
                "refs/heads/topic",
                "true",
                false,
                false,
            ),
            ("pull_request", "refs/heads/trunk", "true", false, false),
        ] {
            let output = Command::new("bash")
                .args(["-euo", "pipefail", "-c", script])
                .env("RELEASE_COMMIT", "a".repeat(40))
                .env("GITHUB_SHA", if same_commit { "a" } else { "b" }.repeat(40))
                .env("DEFAULT_BRANCH", "trunk")
                .env("GITHUB_REF", reference)
                .env("GITHUB_EVENT_NAME", event)
                .env("RELEASE_RECOVERY", recovery)
                .output()?;
            ensure!(
                output.status.success() == expected,
                "incorrect {operation} recovery trigger: {event}/{reference}/{recovery}/{same_commit}"
            );
        }
    }
    for workflow in ["consumer-codeql", "consumer-trivy"] {
        let workflow = load(root, workflow)?;
        let jobs = workflow["jobs"].as_object().context("security jobs")?;
        let checkout = jobs
            .values()
            .filter_map(|job| job["steps"].as_array())
            .flatten()
            .find(|step| step["id"] == "checkout")
            .context("security checkout")?;
        ensure!(
            checkout["with"]["ref"] == "${{ inputs.source-ref }}",
            "recovery security analysis must check out its approved source"
        );
    }
    Ok(())
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
