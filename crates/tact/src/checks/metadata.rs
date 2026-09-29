use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

const UPSTREAM_ACTIONS: [&str; 11] = [
    "peter-evans/create-pull-request",
    "actions/create-github-app-token",
    "actions/upload-artifact",
    "github/codeql-action/init",
    "github/codeql-action/analyze",
    "github/codeql-action/autobuild",
    "github/codeql-action/upload-sarif",
    "actions/cache",
    "runs-on/cache",
    "cachix/install-nix-action",
    "actions/checkout",
];
pub(crate) fn pinned_upstream(reference: &str) -> bool {
    // Revisions live in YAML so Dependabot can update them without a second pin list.
    reference.split_once('@').is_some_and(|(action, revision)| {
        UPSTREAM_ACTIONS.contains(&action)
            && revision.len() == 40
            && revision.chars().all(|c| c.is_ascii_hexdigit())
    })
}
fn load(path: impl AsRef<Path>) -> Result<Value> {
    Ok(serde_norway::from_str(&fs::read_to_string(path)?)?)
}

pub(super) fn adapter(root: &Path) -> Result<()> {
    let a = load(root.join("composite/setup-cache/scripts/cache/action.yaml"))?;
    let steps = &a["runs"]["steps"];
    ensure!(
        steps[0]["if"] == "inputs.backend == 'github'",
        "GitHub backend must be exclusive"
    );
    ensure!(
        steps[1]["if"] == "inputs.backend == 's3'",
        "S3 backend must be exclusive"
    );
    ensure!(
        steps[1]["continue-on-error"] == true,
        "S3 must remain nonfatal"
    );
    ensure!(
        steps[1]["env"]["RUNS_ON_RUNNER_NAME"] == "",
        "ambient RunsOn routing must be disabled"
    );
    ensure!(
        steps[0]["env"]["AWS_ACCESS_KEY_ID"].is_null(),
        "S3 credentials leaked into GitHub backend"
    );
    ensure!(
        !steps.to_string().contains("save-always"),
        "cache saves must be success-only"
    );
    Ok(())
}

pub(super) fn run(root: &Path) -> Result<()> {
    super::releases::contracts(root)?;
    super::workflows::contracts(root)?;
    let suites = crate::manifest::discover(root, None)?;
    for suite in &suites {
        let name = suite.action.as_str();
        super::examples::check(root, name)?;
        let public_name = name
            .strip_prefix("composite/")
            .context("action collection")?;
        let owner = format!(
            "composite/{}",
            public_name.split('/').next().context("action owner")?
        );
        ensure!(
            suite
                .manifest
                .sources
                .iter()
                .all(|source| source.starts_with(&format!("{owner}/"))),
            "test sources must belong to their public action: {name}"
        );
        let a = load(root.join(name).join("action.yaml"))?;
        if !public_name.contains('/') {
            ensure!(
                a["runs"]["using"] == "composite" && root.join(name).join("README.md").is_file(),
                "public contract: {name}"
            );
        }
        if a["runs"]["using"] == "node24" {
            ensure!(
                root.join(name)
                    .join(a["runs"]["main"].as_str().context("Node entrypoint")?)
                    .is_file(),
                "missing Node entrypoint"
            );
            continue;
        }
        let steps = a["runs"]["steps"].as_array().context("composite steps")?;
        for step in steps {
            if let Some(run) = step["run"].as_str() {
                ensure!(
                    step["shell"] == "bash" && !run.contains("${{") && !run.contains("../"),
                    "unsafe inline shell: {name}"
                );
            }
            if let Some(reference) = step["uses"].as_str() {
                if let Some(local) = reference.strip_prefix("$/") {
                    ensure!(
                        local
                            .strip_prefix("composite/")
                            .is_some_and(|path| !path.contains('/'))
                            || local.starts_with(&format!("{owner}/scripts/")),
                        "private helper must belong to its caller: {name} -> {local}"
                    );
                    let target = load(root.join(local).join("action.yaml"))?;
                    if let Some(inputs) = step["with"].as_object() {
                        for key in inputs.keys() {
                            ensure!(
                                target["inputs"].get(key).is_some(),
                                "unknown input {local}/{key}"
                            );
                        }
                    }
                } else {
                    ensure!(
                        pinned_upstream(reference),
                        "expected an allowed upstream action pinned to a full SHA: {reference}"
                    );
                }
            }
            let text = step.to_string();
            for rest in text.split("inputs.").skip(1) {
                let key: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                    .collect();
                ensure!(
                    a["inputs"].get(&key).is_some(),
                    "unknown {name} input: {key}"
                );
            }
        }
        if ["composite/setup-cache", "composite/setup-devenv"].contains(&name) {
            ensure!(
                steps.iter().any(|s| s["uses"] == "$/composite/setup-nix"),
                "missing same-revision Nix prerequisite"
            );
            ensure!(
                !steps
                    .iter()
                    .any(|s| s["uses"] == "$/composite/free-disk-space"),
                "implicit cleanup"
            );
        }
        if ["composite/setup-devenv", "composite/setup-trivy"].contains(&name) {
            ensure!(
                !steps
                    .iter()
                    .any(|s| s["uses"].as_str().is_some_and(|s| s.contains("cache"))),
                "implicit cache setup"
            );
        }
        if name == "composite/setup-cache" {
            ensure!(
                !a["outputs"].to_string().contains("fromJSON("),
                "post hooks cannot reevaluate JSON outputs"
            );
        }
        if name == "composite/run-devenv" {
            ensure!(
                a["outputs"]["result"]["value"] == "${{ steps.result.outputs.result }}"
                    && steps[0]["uses"] == "$/composite/run-devenv/scripts/result"
                    && steps[0]["with"]["phase"] == "prepare"
                    && steps[1]["env"]["DEVENV_RESULT_FILE"]
                        == "${{ steps.result-file.outputs.path }}"
                    && steps[2]["uses"] == "$/composite/run-devenv/scripts/result"
                    && steps[2]["if"] == "always() && steps.result-file.outcome == 'success'"
                    && steps[2]["with"]["outcome"] == "${{ steps.run.outcome }}",
                "result allocation, publication and failure cleanup must stay wired to the command step"
            );
        }
    }
    adapter(root)?;
    let ci = load(root.join(".github/workflows/repository-ci.yaml"))?;
    ensure!(
        ci["jobs"]["cache-warm"]["needs"] == "cache-cold",
        "cold/warm lifecycle dependency"
    );
    ensure!(
        ci["jobs"]["flakes"]["strategy"]["matrix"]["shell"] == json!(["default", "named"]),
        "flake shell matrix"
    );
    ensure!(
        ci["jobs"]["s3-cache-warm"]["needs"] == "s3-cache-cold",
        "S3 post-save must finish before warm restoration"
    );
    for (name, job) in ci["jobs"].as_object().context("CI jobs")? {
        if name == "security" {
            ensure!(
                job["uses"] == "./.github/workflows/test-security-workflows.yaml"
                    && job["permissions"]["security-events"] == "write",
                "reusable security workflows must run within the main CI gate"
            );
        } else if name == "self-hosted" || name.starts_with("s3-cache-") {
            ensure!(
                job["runs-on"]["group"] == "enterprise/tars-cloud",
                "enterprise runner group"
            );
            ensure!(
                !job["steps"]
                    .as_array()
                    .context("steps")?
                    .iter()
                    .any(|s| s["uses"] == "./composite/free-disk-space"),
                "persistent runner cleanup"
            );
        } else {
            ensure!(
                job["strategy"]["matrix"]["runner"] == json!(["ubuntu-24.04", "ubuntu-24.04-arm"]),
                "native architecture matrix: {name}"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependabot_can_update_allowed_actions_to_new_full_shas() {
        let revision = "2892aa5e19bbd11bc0cff5427e3b750a04d9e3c2";
        for action in [
            "github/codeql-action/init",
            "github/codeql-action/analyze",
            "github/codeql-action/autobuild",
            "github/codeql-action/upload-sarif",
            "actions/cache",
            "runs-on/cache",
            "cachix/install-nix-action",
            "actions/checkout",
        ] {
            assert!(pinned_upstream(&format!("{action}@{revision}")), "{action}");
        }
    }

    #[test]
    fn upstream_actions_reject_mutable_refs_and_unapproved_sources() {
        for reference in [
            "actions/checkout",
            "actions/checkout@",
            "actions/checkout@v7",
            "actions/checkout@main",
            "actions/checkout@2892aa5",
            "actions/checkout@gggggggggggggggggggggggggggggggggggggggg",
            "actions/checkout@2892aa5e19bbd11bc0cff5427e3b750a04d9e3c2/extra",
            "unapproved/checkout@2892aa5e19bbd11bc0cff5427e3b750a04d9e3c2",
            "github/codeql-action/unknown@2892aa5e19bbd11bc0cff5427e3b750a04d9e3c2",
        ] {
            assert!(!pinned_upstream(reference), "{reference}");
        }
    }
}
