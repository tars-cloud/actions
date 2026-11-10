use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::path::Path;

const DB_REPOSITORIES: [&str; 4] = [
    "ghcr.io/tars-cloud/actions/trivy-db:2",
    "ghcr.io/aquasecurity/trivy-db:2",
    "docker.io/aquasec/trivy-db:2",
    "public.ecr.aws/aquasecurity/trivy-db:2",
];

const JAVA_DB_REPOSITORIES: [&str; 4] = [
    "ghcr.io/tars-cloud/actions/trivy-java-db:1",
    "ghcr.io/aquasecurity/trivy-java-db:1",
    "docker.io/aquasec/trivy-java-db:1",
    "public.ecr.aws/aquasecurity/trivy-java-db:1",
];

const CHECKS_REPOSITORY: &str = "ghcr.io/tars-cloud/actions/trivy-checks:2";

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
    self_scan_configs(root, &security)
}

fn self_scan_configs(root: &Path, security: &Value) -> Result<()> {
    for field in ["config-file", "gate-config-file"] {
        let path = security["jobs"]["trivy"]["with"][field]
            .as_str()
            .context("Trivy configuration")?;
        self_scan_policy(field, &super::load(root, path)?)?;
    }
    let integration = super::load(root, ".github/workflows/test-security-workflows.yaml")?;
    mirror_callers(security, &integration)
}

fn self_scan_policy(field: &str, config: &Value) -> Result<()> {
    ensure!(
        config["scan"]["scanners"]
            .as_array()
            .context("scanners")?
            .iter()
            .any(|value| value == "vuln"),
        "self-scans must include vulnerability detection"
    );
    ensure!(
        config["db"]["repository"] == json!(DB_REPOSITORIES)
            && config["java-db"]["repository"] == json!(JAVA_DB_REPOSITORIES)
            && config["misconfiguration"]["checks-bundle-repository"] == CHECKS_REPOSITORY,
        "self-scan Trivy configurations must prefer published TARS Cloud mirrors"
    );
    if field == "gate-config-file" {
        ensure!(
            config["severity"] == json!(["HIGH", "CRITICAL"])
                && config["scan"]["scanners"] == json!(["vuln", "misconfig", "secret"]),
            "gate HIGH/CRITICAL vulnerabilities, misconfigurations and secrets"
        );
    }
    Ok(())
}

fn mirror_callers(secops: &Value, integration: &Value) -> Result<()> {
    for call in [
        &secops["jobs"]["trivy"],
        &integration["jobs"]["trivy-direct"],
        &integration["jobs"]["trivy-flakes"],
    ] {
        ensure!(
            call["with"]
                .get("use-database-mirrors")
                .unwrap_or(&json!(true))
                == &json!(true),
            "repository Trivy workflows must use the published database mirrors"
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn self_scan_policy_rejects_bootstrap_only_sources_and_weak_gates() {
        let root = root();
        let report = super::super::load(&root, "trivy.yaml").unwrap();
        let gate = super::super::load(&root, ".github/trivy/gate.yaml").unwrap();
        self_scan_policy("config-file", &report).unwrap();
        self_scan_policy("gate-config-file", &gate).unwrap();

        let mut without_vuln = report.clone();
        without_vuln["scan"]["scanners"] = json!(["secret"]);
        assert!(self_scan_policy("config-file", &without_vuln).is_err());

        for pointer in [
            "/db/repository",
            "/java-db/repository",
            "/misconfiguration/checks-bundle-repository",
        ] {
            let mut changed = report.clone();
            *changed.pointer_mut(pointer).unwrap() = json!(["ghcr.io/aquasecurity/trivy-db:2"]);
            assert!(self_scan_policy("config-file", &changed).is_err());
        }

        let mut weak_gate = gate;
        weak_gate["severity"] = json!(["CRITICAL"]);
        assert!(self_scan_policy("gate-config-file", &weak_gate).is_err());
    }

    #[test]
    fn mirror_callers_reject_bootstrap_opt_outs() {
        let root = root();
        let secops = super::super::load(&root, ".github/workflows/repository-secops.yaml").unwrap();
        let integration =
            super::super::load(&root, ".github/workflows/test-security-workflows.yaml").unwrap();
        mirror_callers(&secops, &integration).unwrap();

        let mut opted_out = secops.clone();
        opted_out["jobs"]["trivy"]["with"]["use-database-mirrors"] = json!(false);
        assert!(mirror_callers(&opted_out, &integration).is_err());

        for job in ["trivy-direct", "trivy-flakes"] {
            let mut changed = integration.clone();
            changed["jobs"][job]["with"]["use-database-mirrors"] = json!(false);
            assert!(mirror_callers(&secops, &changed).is_err());
        }
    }
}
