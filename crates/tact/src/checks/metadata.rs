use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

const PINS: [(&str, &str); 5] = [
    ("actions/cache", "55cc8345863c7cc4c66a329aec7e433d2d1c52a9"),
    ("runs-on/cache", "88d90644011a3a9957fd141a106f5a94f9794203"),
    (
        "cachix/install-nix-action",
        "13d8dd58da0234aa297dedd986986ccb8e7f3e24",
    ),
    (
        "cachix/cachix-action",
        "38b082610b782e7e93e209c35fd730d399dee866",
    ),
    (
        "actions/checkout",
        "3d3c42e5aac5ba805825da76410c181273ba90b1",
    ),
];
fn load(path: impl AsRef<Path>) -> Result<Value> {
    Ok(serde_norway::from_str(&fs::read_to_string(path)?)?)
}

pub(super) fn adapter(root: &Path) -> Result<()> {
    let a = load(root.join("composite/setup-cache/scripts/cache/action.yml"))?;
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
    let suites = crate::manifest::discover(root, None)?;
    for suite in &suites {
        let name = suite.action.as_str();
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
        let a = load(root.join(name).join("action.yml"))?;
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
                    let target = load(root.join(local).join("action.yml"))?;
                    if let Some(inputs) = step["with"].as_object() {
                        for key in inputs.keys() {
                            ensure!(
                                target["inputs"].get(key).is_some(),
                                "unknown input {local}/{key}"
                            );
                        }
                    }
                } else {
                    let (owner, sha) = reference
                        .split_once('@')
                        .context("unpinned upstream action")?;
                    ensure!(PINS.contains(&(owner, sha)), "unreviewed pin: {reference}");
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
    }
    adapter(root)?;
    let ci = load(root.join(".github/workflows/ci.yml"))?;
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
        if name == "self-hosted" || name.starts_with("s3-cache-") {
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
