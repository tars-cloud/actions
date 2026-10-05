use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{path::Path, process::Command};

pub(super) fn evaluate_expression(
    source: &str,
    inputs: Value,
    matrix: Value,
    vars: Value,
) -> Result<Value> {
    let source = source
        .strip_prefix("${{")
        .and_then(|value| value.strip_suffix("}}"))
        .context("runner expression")?;
    let fixture = json!({"source":source,"inputs":inputs,"matrix":matrix,"vars":vars});
    let output = Command::new(crate::runner::executable("node")?).args([
        "-e",
        r#"const {source,inputs,matrix,vars}=JSON.parse(process.argv[1]); const needs={"s3-cache-cold":{outputs:{arch:matrix.architecture==='ARM64'?'ARM64':'X64'}}}; const fromJSON=JSON.parse; const toJSON=JSON.stringify; const expression=source.replace(/\b(inputs|needs)\.([a-z0-9-]+)/g,(_,context,key)=>`${context}[${JSON.stringify(key)}]`); process.stdout.write(JSON.stringify(eval(expression)));"#,
        &serde_json::to_string(&fixture)?,
    ]).output()?;
    ensure!(
        output.status.success(),
        "runner expression failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

pub(super) fn runners(root: &Path) -> Result<()> {
    for entry in std::fs::read_dir(root.join(".github/workflows"))? {
        let entry = entry?;
        if !entry.file_name().to_string_lossy().starts_with("consumer-") {
            continue;
        }
        let workflow = super::workflows::load(
            root,
            &format!(".github/workflows/{}", entry.file_name().to_string_lossy()),
        )?;
        for job in workflow["jobs"]
            .as_object()
            .context("consumer jobs")?
            .values()
        {
            let source = job["runs-on"].as_str().context("configurable runner")?;
            for selector in [
                json!("ubuntu-24.04-arm"),
                json!(["self-hosted", "linux", "arm64"]),
                json!({"group":"enterprise/example","labels":["self-hosted","linux","x64"]}),
            ] {
                let inputs =
                    json!({"runs-on":serde_json::to_string(&selector)?,"reporting-runs-on":""});
                ensure!(
                    evaluate_expression(source, inputs, json!({"language":"actions"}), json!({}))?
                        == selector,
                    "every job must honor string, label and group selectors"
                );
            }
            ensure!(
                evaluate_expression(
                    source,
                    json!({"runs-on":"","reporting-runs-on":""}),
                    json!({"language":"actions"}),
                    json!({})
                )? == json!("ubuntu-24.04"),
                "empty selectors must retain hosted defaults"
            );
            if source.contains("reporting-runs-on") {
                ensure!(
                    evaluate_expression(
                        source,
                        json!({"runs-on":"[\"self-hosted\"]","reporting-runs-on":"\"ubuntu-24.04\""}),
                        json!({}),
                        json!({})
                    )? == json!("ubuntu-24.04"),
                    "explicit reporting override must win"
                );
            }
        }
    }
    Ok(())
}

pub(super) fn run(root: &Path) -> Result<()> {
    runners(root)?;
    super::runners::run(root)?;
    let workflow = super::workflows::load(root, ".github/workflows/consumer-devenv-ci.yaml")?;
    let test = super::workflows::step(&workflow, "ci", "run_tests")?;
    ensure!(
        test["if"] == "always() && steps.set_up_consumer.outcome == 'success'",
        "tests must run after failed lint, but not after failed setup"
    );
    ensure!(
        test["with"]["mode"]
            .as_str()
            .is_some_and(|value| value.contains("inputs.test-command == ''")
                && value.contains("inputs.type == 'devenv'")),
        "only default direct tests may use configured test mode"
    );
    for (id, field) in [("run_lint", "LINT_COMMAND"), ("run_tests", "TEST_COMMAND")] {
        super::workflows::execute(
            root,
            &workflow,
            "ci",
            id,
            json!({"env":{field:"printf 'literal $(touch injected)' > result"},"expect":{"exit":0,"calls":[],"files":{"result":"literal $(touch injected)","injected":null}}}),
        )?;
        super::workflows::execute(
            root,
            &workflow,
            "ci",
            id,
            json!({"env":{field:"exit 7"},"expect":{"exit":7,"calls":[]}}),
        )?;
    }
    super::workflows::execute(
        root,
        &workflow,
        "ci",
        "run_tests",
        json!({"env":{"TEST_COMMAND":"","CONSUMER_TYPE":"flakes"},"expect":{"exit":1,"calls":[]}}),
    )?;
    Ok(())
}
