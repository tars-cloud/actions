use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::path::Path;

const SECRETS: [&str; 6] = [
    "S3_ENDPOINT",
    "S3_BUCKET",
    "S3_REGION",
    "S3_ACCESS_KEY",
    "S3_SECRET_ACCESS_KEY",
    "S3_SESSION_TOKEN",
];

pub(super) fn run(root: &Path) -> Result<()> {
    for name in [
        "test-security-workflows",
        "test-consumer-ci",
        "test-cargo-crap",
        "consumer-codeql",
        "consumer-cargo-crap",
    ] {
        let workflow = super::load(root, &format!(".github/workflows/{name}.yaml"))?;
        for secret in SECRETS {
            ensure!(
                workflow["on"]["workflow_call"]["secrets"][secret].is_object(),
                "{name} must declare {secret}"
            );
        }
    }
    for name in [
        "repository-ci",
        "repository-cargo-crap",
        "test-security-workflows",
        "test-consumer-ci",
        "test-cargo-crap",
    ] {
        let workflow = super::load(root, &format!(".github/workflows/{name}.yaml"))?;
        check_calls(root, &workflow)?;
    }
    for (name, checkout, environment) in [
        ("consumer-codeql", "checkout", "devenv"),
        (
            "consumer-cargo-crap",
            "check_out_repository",
            "set_up_devenv",
        ),
    ] {
        check_restore(root, name, checkout, environment)?;
    }
    Ok(())
}

fn check_restore(root: &Path, name: &str, checkout: &str, environment: &str) -> Result<()> {
    let workflow = super::load(root, &format!(".github/workflows/{name}.yaml"))?;
    let steps = workflow["jobs"]["analyze"]["steps"]
        .as_array()
        .context("analysis steps")?;
    let position = |id: &str| steps.iter().position(|step| step["id"] == id);
    ensure!(
        position("restore_cache").context("cache step")?
            < position(environment).context("environment step")?,
        "{name} must restore archives before building the environment"
    );
    let cache = steps
        .iter()
        .find(|step| step["id"] == "restore_cache")
        .unwrap();
    ensure!(
        cache["with"]["s3-endpoint"].as_str().is_some_and(|value| {
            value.contains(&format!("steps.{checkout}.outputs.trusted"))
                && value.contains("secrets.S3_ENDPOINT")
        }),
        "{name} must forward S3 configuration using its checkout trust result"
    );
    Ok(())
}

fn check_calls(root: &Path, workflow: &Value) -> Result<()> {
    for job in workflow["jobs"].as_object().context("jobs")?.values() {
        let Some(reference) = job["uses"].as_str() else {
            continue;
        };
        let target = super::load(root, reference.trim_start_matches("./"))?;
        if target["on"]["workflow_call"]["secrets"]["S3_ENDPOINT"].is_null() {
            continue;
        }
        check_secrets(job)?;
    }
    Ok(())
}

fn check_secrets(job: &Value) -> Result<()> {
    if job["secrets"] == "inherit" {
        return Ok(());
    }
    for secret in SECRETS {
        ensure!(
            job["secrets"][secret] == format!("${{{{ secrets.{secret} }}}}"),
            "{} must forward {secret}",
            job["uses"]
        );
    }
    Ok(())
}
