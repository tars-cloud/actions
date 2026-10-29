use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::path::Path;

pub(super) fn contracts(root: &Path) -> Result<()> {
    crate::config::load(&root.join("tact.yaml"))?;
    let mirror = super::load(root, ".github/workflows/repository-mirrors.yaml")?;
    ensure!(
        mirror["on"]["schedule"][0]["cron"] == "0 */6 * * *"
            && mirror["on"].get("workflow_dispatch").is_some()
            && mirror["on"].get("pull_request").is_none()
            && mirror["on"].get("pull_request_target").is_none(),
        "mirror publication must be scheduled or manually dispatched"
    );
    ensure!(
        mirror["jobs"]["refresh"]["if"]
            == "github.repository == 'tars-cloud/actions' && github.ref == 'refs/heads/trunk'"
            && mirror["jobs"]["refresh"]["permissions"]["packages"] == "write"
            && mirror["jobs"]["refresh"]["permissions"]["contents"] == "read"
            && mirror["concurrency"]["cancel-in-progress"] == false,
        "mirror publication must serialize trusted trunk runs with package-only write access"
    );
    ensure!(
        super::step(&mirror, "refresh", "set_up_devenv")?["uses"] == "./composite/setup-devenv"
            && super::step(&mirror, "refresh", "refresh_mirrors")?["uses"]
                == "./composite/run-devenv",
        "mirror tools must run through this revision's devenv actions"
    );

    let security = super::load(root, ".github/workflows/repository-secops.yaml")?;
    for (job, workflow) in [("trivy", "consumer-trivy"), ("codeql", "consumer-codeql")] {
        let call = &security["jobs"][job];
        let path = format!(".github/workflows/{workflow}.yaml");
        ensure!(
            call["uses"] == format!("./{path}") && call["with"]["type"] == "devenv",
            "self-scans must use the current revision and direct devenv"
        );
        super::inputs(call, &super::load(root, &path)?["on"]["workflow_call"])?;
    }
    for field in ["config-file", "gate-config-file"] {
        let path = security["jobs"]["trivy"]["with"][field]
            .as_str()
            .context("Trivy configuration")?;
        let config = super::load(root, path)?;
        ensure!(
            config["scan"]["scanners"]
                .as_array()
                .context("scanners")?
                .iter()
                .any(|value| value == "vuln"),
            "self-scans must include vulnerability detection"
        );
        if field == "gate-config-file" {
            ensure!(
                config["severity"] == json!(["HIGH", "CRITICAL"])
                    && config["scan"]["scanners"] == json!(["vuln", "misconfig", "secret"]),
                "gate HIGH/CRITICAL vulnerabilities, misconfigurations and secrets"
            );
        }
    }
    Ok(())
}

pub(super) fn scripts(root: &Path) -> Result<()> {
    let workflow = super::load(root, ".github/workflows/repository-mirrors.yaml")?;
    let script = super::step(&workflow, "refresh", "refresh_mirrors")?["with"]["run"]
        .as_str()
        .context("mirror script")?;
    for (login, mirror, expected) in [(0, 0, 0), (9, 0, 9), (0, 7, 7)] {
        let command = format!(
            r#"
oras() {{
  [[ "$*" != *fixture-token* ]]
  IFS= read -r token || true
  test "$token" = fixture-token
  printf auth > "$TACT_ORAS_CONFIG"
  return {login}
}}
tact() {{
  test "$*" = mirror
  test -f "$TACT_ORAS_CONFIG"
  return {mirror}
}}
set +e
(
set -e
{script}
)
status=$?
set -e
test "$status" = {expected}
shopt -s nullglob
remaining=("$RUNNER_TEMP"/tact-mirror-auth-*)
test "${{#remaining[@]}}" = 0
"#
        );
        let case: Value = json!({
            "id":"mirror-auth-cleanup", "description":"Verify temporary registry credentials are removed on success, login failure and refresh failure",
            "command":["bash","-euo","pipefail","-c",command], "tools":["mktemp","rm"],
            "env":{"RUNNER_TEMP":"${state}/tmp","GH_TOKEN":"fixture-token","GITHUB_ACTOR":"fixture"},
            "expect":{"exit":0,"calls":[]}
        });
        crate::runner::run(
            root,
            &serde_json::from_value(json!({"version":1,"tests":[]}))?,
            &serde_json::from_value(case)?,
        )?;
    }
    Ok(())
}
