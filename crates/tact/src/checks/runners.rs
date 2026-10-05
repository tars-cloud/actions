use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::path::Path;

fn policy(source: &str, architecture: &str, kind: &str, selectors: bool) -> Result<Value> {
    let mut variables = json!({"TARS_CLOUD_RUNNER_TYPE":kind});
    if selectors {
        variables["TARS_CLOUD_RUNNER_AMD64"] =
            json!(r#"{"group":"enterprise/example","labels":["linux","x64"]}"#);
        variables["TARS_CLOUD_RUNNER_ARM64"] = variables["TARS_CLOUD_RUNNER_AMD64"].clone();
    }
    super::consumer_ci::evaluate_expression(
        source,
        json!({"runs-on":"","reporting-runs-on":"","runner-architecture":architecture,"system":""}),
        json!({"language":"actions","architecture":architecture}),
        variables,
    )
}

fn defaults(source: &str) -> Result<()> {
    for (architecture, hosted, native) in [
        ("AMD64", "ubuntu-24.04", "x64"),
        ("ARM64", "ubuntu-24.04-arm", "ARM64"),
    ] {
        for kind in ["", "saas"] {
            ensure!(
                policy(source, architecture, kind, true)? == json!(hosted),
                "SaaS must ignore configured self-hosted selectors: {architecture}/{kind}"
            );
        }
        ensure!(
            policy(source, architecture, "self-hosted", false)?
                == json!(["self-hosted", "linux", native]),
            "missing self-hosted selectors must request native hardware: {architecture}"
        );
        ensure!(
            policy(source, architecture, "self-hosted", true)?
                == json!({"group":"enterprise/example","labels":["linux","x64"]}),
            "explicit ARM64 emulation selectors must be honored"
        );
        let selector = json!({"group":"enterprise/example","labels":["linux","x64"]});
        let vars = json!({
            "TARS_CLOUD_RUNNER_TYPE":"self-hosted",
            "TARS_CLOUD_RUNNER_AMD64":serde_json::to_string(&selector)?,
        });
        ensure!(
            super::consumer_ci::evaluate_expression(
                source,
                json!({"runs-on":"","reporting-runs-on":"","runner-architecture":architecture}),
                json!({"language":"actions","architecture":architecture}),
                vars,
            )? == if architecture == "ARM64" {
                json!(["self-hosted", "linux", "ARM64"])
            } else {
                selector
            },
            "ARM64 must not inherit a configured AMD64 selector"
        );
    }
    Ok(())
}

pub(super) fn run(root: &Path) -> Result<()> {
    repository_systems(root)?;
    for entry in std::fs::read_dir(root.join(".github/workflows"))? {
        let entry = entry?;
        let consumer = entry.file_name().to_string_lossy().starts_with("consumer-");
        let workflow = super::workflows::load(
            root,
            &format!(".github/workflows/{}", entry.file_name().to_string_lossy()),
        )?;
        for (name, job) in workflow["jobs"].as_object().context("consumer jobs")? {
            let Some(source) = job["runs-on"].as_str() else {
                continue;
            };
            if consumer || source.contains("matrix.architecture") || source.contains("needs.") {
                defaults(source)?;
            }
            if consumer {
                systems(job)?;
            }
            for (kind, architecture, exit) in [
                ("", "AMD64", 0),
                ("saas", "ARM64", 0),
                ("self-hosted", "ARM64", 0),
                ("selfhosted", "AMD64", 1),
                ("SAAS", "AMD64", 1),
                ("saas", "other", 1),
            ] {
                super::workflows::execute(
                    root,
                    &workflow,
                    name,
                    "validate_runner",
                    json!({"env":{"RUNNER_TYPE":kind,"RUNNER_ARCHITECTURE":architecture},"expect":{"exit":exit,"calls":[]}}),
                )?;
            }
        }
    }
    Ok(())
}

fn repository_systems(root: &Path) -> Result<()> {
    let workflow = super::workflows::load(root, ".github/workflows/repository-ci.yaml")?;
    for (job, id) in [
        ("consumer", "run_remote_consumer"),
        ("direct", "set_up_devenv"),
        ("direct", "reinstall_devenv"),
        ("direct", "set_up_trivy"),
        ("direct", "validate_repository"),
        ("direct", "run_without_result"),
        ("direct", "skip_result"),
        ("direct", "run_integration_checks"),
        ("direct", "run_devenv_tests"),
        ("flakes", "restore_cache"),
        ("flakes", "set_up_devenv"),
        ("flakes", "set_up_trivy"),
        ("flakes", "verify_shell"),
        ("cache-cold", "restore_cache"),
        ("cache-warm", "restore_cache"),
    ] {
        let step = super::workflows::step(&workflow, job, id)?;
        let source = step["with"]["system"].as_str().context("matrix system")?;
        for (architecture, system) in [("AMD64", "x86_64-linux"), ("ARM64", "aarch64-linux")] {
            ensure!(
                super::consumer_ci::evaluate_expression(
                    source,
                    json!({}),
                    json!({"architecture":architecture,"runner":if architecture == "ARM64" {"ubuntu-24.04-arm"} else {"ubuntu-24.04"}}),
                    json!({}),
                )? == json!(system),
                "repository {job}/{id} must preserve the requested architecture"
            );
        }
    }
    for (measured, exit) in [("aarch64-linux", 0), ("x86_64-linux", 1)] {
        super::workflows::execute(
            root,
            &workflow,
            "direct",
            "verify_environment_outputs",
            json!({
                "env":{"DEVENV_VERSION":"1.0.0","SELECTED_SYSTEM":measured,"EXPECTED_SYSTEM":"aarch64-linux"},
                "commands":{
                    "devenv":[{"args":["--no-tui","--version"],"stdout":"devenv 1.0.0\n","exit":0}],
                },
                "expect":{"exit":exit,"calls":[
                    {"command":"devenv","args":["--no-tui","--version"]},
                ]},
            }),
        )?;
    }
    Ok(())
}

fn systems(job: &Value) -> Result<()> {
    for step in job["steps"].as_array().context("workflow steps")? {
        let action = step["uses"].as_str().unwrap_or("");
        if ![
            "$/composite/setup-consumer",
            "$/composite/setup-devenv",
            "$/composite/setup-cache",
            "$/composite/setup-trivy",
            "$/composite/run-devenv",
            "$/composite/release-rust",
            "$/composite/cargo-crap",
        ]
        .contains(&action)
        {
            continue;
        }
        let source = step["with"]["system"].as_str().context("consumer system")?;
        for (requested, expected) in [("", "aarch64-linux"), ("x86_64-linux", "x86_64-linux")] {
            ensure!(
                super::consumer_ci::evaluate_expression(
                    source,
                    json!({"system":requested,"runner-architecture":"ARM64"}),
                    json!({}),
                    json!({}),
                )? == json!(expected),
                "ARM64 execution must select its environment and preserve explicit system overrides: {action}"
            );
        }
    }
    Ok(())
}
