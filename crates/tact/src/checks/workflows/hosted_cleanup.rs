use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::path::Path;

const ENABLED: &str = "inputs.cleanup-android || inputs.cleanup-dotnet || inputs.cleanup-haskell || inputs.cleanup-boost || inputs.cleanup-swift";

fn normalized(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn contract(workflow: &Value, job_id: &str, before: &[&str]) -> Result<()> {
    let cleanup = super::step(workflow, job_id, "free_disk_space")?;
    ensure!(
        cleanup["uses"] == "$/composite/free-disk-space"
            && cleanup.get("continue-on-error").is_none(),
        "cleanup must compose the existing primitive at the revision under test and propagate failures"
    );
    ensure!(
        normalized(cleanup["if"].as_str().context("cleanup condition")?)
            == format!("runner.os == 'Linux' && ({ENABLED})"),
        "cleanup requires actual Linux runner context and explicit opt-in"
    );
    for category in ["android", "dotnet", "haskell", "boost", "swift"] {
        let input = format!("cleanup-{category}");
        let definition = &workflow["on"]["workflow_call"]["inputs"][&input];
        ensure!(
            definition["type"] == "boolean" && definition["default"] == false,
            "cleanup must preserve SDKs by default: {category}"
        );
        ensure!(
            cleanup["with"][category] == format!("${{{{ inputs.{input} }}}}"),
            "forward every cleanup category explicitly: {category}"
        );
    }
    let steps = workflow["jobs"][job_id]["steps"]
        .as_array()
        .context("analysis steps")?;
    let position = |id| -> Result<usize> {
        steps
            .iter()
            .position(|step| step["id"] == id)
            .context("cleanup ordering step")
    };
    for id in before {
        ensure!(
            position("free_disk_space")? < position(id)?,
            "cleanup must precede {id}"
        );
    }
    for (id, job) in workflow["jobs"].as_object().context("workflow jobs")? {
        if id != job_id {
            ensure!(
                job["steps"]
                    .as_array()
                    .context("reporting steps")?
                    .iter()
                    .all(|step| step["uses"] != "$/composite/free-disk-space"),
                "reporting jobs must not remove SDKs"
            );
        }
    }
    Ok(())
}

pub(super) fn contracts(root: &Path) -> Result<()> {
    let crap = super::load(root, ".github/workflows/consumer-cargo-crap.yaml")?;
    contract(
        &crap,
        "analyze",
        &["restore_cache", "set_up_devenv", "run_analysis"],
    )?;
    let report = super::step(&crap, "analyze", "report_status")?;
    ensure!(
        report["with"]["allow-skipped"] == "cleanup"
            && report["with"]["results"]
                .as_str()
                .context("status results")?
                .contains("cleanup=${{ steps.free_disk_space.outcome }}"),
        "report cleanup failures and allow an opt-out skip"
    );
    let codeql = super::load(root, ".github/workflows/consumer-codeql.yaml")?;
    contract(
        &codeql,
        "analyze",
        &[
            "restore_cache",
            "devenv",
            "setup",
            "setup_environment",
            "init",
        ],
    )?;
    let validation = super::step(&codeql, "analyze", "validate_cleanup")?;
    ensure!(
        normalized(
            validation["env"]["CLEANUP_ENABLED"]
                .as_str()
                .context("cleanup validation input")?
        ) == format!("${{{{ {ENABLED} }}}}"),
        "validate every selected category on non-Linux CodeQL runners"
    );
    let steps = codeql["jobs"]["analyze"]["steps"]
        .as_array()
        .context("CodeQL steps")?;
    ensure!(
        steps.iter().position(|s| s["id"] == "validate_cleanup")
            < steps.iter().position(|s| s["id"] == "free_disk_space"),
        "validate cleanup before deleting SDKs"
    );
    let trivy = super::load(root, ".github/workflows/consumer-trivy.yaml")?;
    trivy_contract(&trivy)?;
    let ci = super::load(root, ".github/workflows/consumer-devenv-ci.yaml")?;
    contract(&ci, "ci", &["set_up_consumer", "run_lint", "run_tests"])?;
    let report = super::step(&ci, "ci", "report_status")?;
    ensure!(
        report["if"] == "always()"
            && report["with"]["fail-on-error"] == "true"
            && report["with"]["allow-skipped"]
                .as_str()
                .context("allowed skipped steps")?
                .lines()
                .any(|line| line == "sdk-cleanup")
            && report["with"]["results"]
                .as_str()
                .context("status results")?
                .contains("sdk-cleanup=${{ steps.free_disk_space.outcome }}"),
        "Devenv CI must report SDK cleanup failures and allow opt-out skips"
    );
    let update = super::load(root, ".github/workflows/consumer-devenv-update.yaml")?;
    contract(
        &update,
        "update",
        &["devenv", "update", "validate", "app-token", "pull-request"],
    )?;
    for (path, jobs) in [
        (".github/workflows/test-cargo-crap.yaml", &["consumer"][..]),
        (
            ".github/workflows/test-consumer-ci.yaml",
            &["native", "self-hosted"][..],
        ),
        (
            ".github/workflows/repository-ci.yaml",
            &["devenv-update"][..],
        ),
        (
            ".github/workflows/test-security-workflows.yaml",
            &["codeql", "codeql-flakes", "trivy-direct", "trivy-flakes"][..],
        ),
    ] {
        let caller = super::load(root, path)?;
        for job in jobs {
            for category in ["android", "dotnet", "haskell", "boost", "swift"] {
                ensure!(
                    caller["jobs"][job]["with"][format!("cleanup-{category}")] == true,
                    "native callers must exercise all SDK categories: {job}/{category}"
                );
            }
        }
    }
    Ok(())
}

fn trivy_contract(workflow: &Value) -> Result<()> {
    contract(workflow, "scan", &["cache", "devenv", "trivy", "scan"])?;
    let report = super::step(workflow, "scan", "report")?;
    ensure!(
        report["if"] == "always()"
            && report["with"]["fail-on-error"] == "true"
            && report["with"]["allow-skipped"]
                .as_str()
                .context("allowed skipped steps")?
                .lines()
                .any(|line| line == "sdk-cleanup")
            && report["with"]["results"]
                .as_str()
                .context("status results")?
                .contains("sdk-cleanup=${{ steps.free_disk_space.outcome }}"),
        "Trivy must report SDK cleanup failures separately from report cleanup and allow opt-out skips"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_implicit_sdk_removal_and_cleanup_after_setup() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let workflow = super::super::load(&root, ".github/workflows/consumer-codeql.yaml").unwrap();
        super::contract(
            &workflow,
            "analyze",
            &["restore_cache", "devenv", "setup", "init"],
        )
        .unwrap();
        for category in ["android", "dotnet", "haskell", "boost", "swift"] {
            let mut changed = workflow.clone();
            changed["on"]["workflow_call"]["inputs"][format!("cleanup-{category}")]["default"] =
                true.into();
            assert!(super::contract(&changed, "analyze", &["init"]).is_err());
            let mut changed = workflow.clone();
            let steps = changed["jobs"]["analyze"]["steps"].as_array_mut().unwrap();
            let cleanup = steps
                .iter_mut()
                .find(|s| s["id"] == "free_disk_space")
                .unwrap();
            cleanup["with"].as_object_mut().unwrap().remove(category);
            assert!(super::contract(&changed, "analyze", &["init"]).is_err());
        }
        let mut changed = workflow;
        let steps = changed["jobs"]["analyze"]["steps"].as_array_mut().unwrap();
        let cleanup = steps
            .iter()
            .position(|s| s["id"] == "free_disk_space")
            .unwrap();
        let step = steps.remove(cleanup);
        steps.push(step);
        assert!(super::contract(&changed, "analyze", &["init"]).is_err());
    }

    #[test]
    fn trivy_rejects_late_cleanup_and_hidden_failures() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let workflow = super::super::load(&root, ".github/workflows/consumer-trivy.yaml").unwrap();
        super::trivy_contract(&workflow).unwrap();
        let mut changed = workflow.clone();
        let steps = changed["jobs"]["scan"]["steps"].as_array_mut().unwrap();
        let cleanup = steps
            .iter()
            .position(|step| step["id"] == "free_disk_space")
            .unwrap();
        let step = steps.remove(cleanup);
        steps.push(step);
        assert!(super::trivy_contract(&changed).is_err());

        for (id, field, value) in [
            ("free_disk_space", "continue-on-error", true.into()),
            ("free_disk_space", "if", "always()".into()),
            ("report", "if", "success()".into()),
            ("report", "with", serde_json::json!({})),
        ] {
            let mut changed = workflow.clone();
            let step = changed["jobs"]["scan"]["steps"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|step| step["id"] == id)
                .unwrap();
            step[field] = value;
            assert!(super::trivy_contract(&changed).is_err());
        }
    }
}
