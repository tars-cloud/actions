use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::path::Path;

const SECRETS: [&str; 3] = [
    "TARS_CLOUD_S3_ACCESS_KEY",
    "TARS_CLOUD_S3_SECRET_ACCESS_KEY",
    "TARS_CLOUD_S3_SESSION_TOKEN",
];

pub(super) fn run(root: &Path) -> Result<()> {
    for name in [
        "test-security-workflows",
        "test-devenv-ci",
        "test-cargo-crap",
        "devenv-ci",
        "trivy",
        "codeql",
        "cargo-crap",
    ] {
        let workflow = super::load(root, &format!(".github/workflows/{name}.yaml"))?;
        for secret in SECRETS {
            ensure!(
                workflow["on"]["workflow_call"]["secrets"][secret].is_object(),
                "{name} must declare {secret}"
            );
        }
        for legacy in [
            "S3_ENDPOINT",
            "S3_BUCKET",
            "S3_REGION",
            "S3_ACCESS_KEY",
            "S3_SECRET_ACCESS_KEY",
            "S3_SESSION_TOKEN",
            "TARS_CLOUD_S3_ENDPOINT",
            "TARS_CLOUD_S3_BUCKET",
            "TARS_CLOUD_S3_REGION",
        ] {
            ensure!(
                workflow["on"]["workflow_call"]["secrets"][legacy].is_null(),
                "{name} must not declare {legacy} as a workflow secret"
            );
        }
    }
    for name in [
        "repository-ci",
        "repository-cargo-crap",
        "repository-secops",
        "test-security-workflows",
        "test-devenv-ci",
        "test-cargo-crap",
    ] {
        let workflow = super::load(root, &format!(".github/workflows/{name}.yaml"))?;
        check_calls(root, &workflow)?;
    }
    for (name, job, checkout, cache_step, environment) in [
        ("codeql", "analyze", "checkout", "restore_cache", "devenv"),
        (
            "cargo-crap",
            "analyze",
            "check_out_repository",
            "restore_cache",
            "set_up_devenv",
        ),
        ("trivy", "scan", "checkout", "cache", "devenv"),
    ] {
        check_restore(root, name, job, checkout, cache_step, environment)?;
    }
    Ok(())
}

fn check_restore(
    root: &Path,
    name: &str,
    job: &str,
    checkout: &str,
    cache_step: &str,
    environment: &str,
) -> Result<()> {
    let workflow = super::load(root, &format!(".github/workflows/{name}.yaml"))?;
    let steps = workflow["jobs"][job]["steps"]
        .as_array()
        .context("workflow steps")?;
    let position = |id: &str| steps.iter().position(|step| step["id"] == id);
    ensure!(
        position(cache_step).context("cache step")?
            < position(environment).context("environment step")?,
        "{name} must restore archives before building the environment"
    );
    let cache = steps.iter().find(|step| step["id"] == cache_step).unwrap();
    for (input, source) in [
        ("s3-endpoint", "vars.TARS_CLOUD_S3_ENDPOINT"),
        ("s3-bucket", "vars.TARS_CLOUD_S3_BUCKET"),
        ("s3-region", "vars.TARS_CLOUD_S3_REGION"),
        ("s3-access-key", "secrets.TARS_CLOUD_S3_ACCESS_KEY"),
        ("s3-secret-key", "secrets.TARS_CLOUD_S3_SECRET_ACCESS_KEY"),
        ("s3-session-token", "secrets.TARS_CLOUD_S3_SESSION_TOKEN"),
    ] {
        ensure!(
            cache["with"][input].as_str().is_some_and(|value| {
                value.contains(&format!("steps.{checkout}.outputs.trusted"))
                    && value.contains(source)
            }),
            "{name} must forward {source} to {input} using its checkout trust result"
        );
    }
    Ok(())
}

fn check_calls(root: &Path, workflow: &Value) -> Result<()> {
    for job in workflow["jobs"].as_object().context("jobs")?.values() {
        let Some(reference) = job["uses"].as_str() else {
            continue;
        };
        let target = super::load(root, reference.trim_start_matches("./"))?;
        if target["on"]["workflow_call"]["secrets"]["TARS_CLOUD_S3_ACCESS_KEY"].is_null() {
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
