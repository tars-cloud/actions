use anyhow::{Context, Result, ensure};
use serde_json::json;
use std::path::Path;

const ARGS: [&str; 18] = [
    "--db-repository",
    "ghcr.io/tars-cloud/actions/trivy-db:2",
    "--java-db-repository",
    "ghcr.io/tars-cloud/actions/trivy-java-db:1",
    "--db-repository",
    "ghcr.io/aquasecurity/trivy-db:2",
    "--java-db-repository",
    "ghcr.io/aquasecurity/trivy-java-db:1",
    "--db-repository",
    "docker.io/aquasec/trivy-db:2",
    "--java-db-repository",
    "docker.io/aquasec/trivy-java-db:1",
    "--db-repository",
    "public.ecr.aws/aquasecurity/trivy-db:2",
    "--java-db-repository",
    "public.ecr.aws/aquasecurity/trivy-java-db:1",
    "--checks-bundle-repository",
    "ghcr.io/tars-cloud/actions/trivy-checks:2",
];

pub(super) fn contracts(root: &Path) -> Result<()> {
    let workflow = super::load(root, ".github/workflows/consumer-trivy.yaml")?;
    let input = &workflow["on"]["workflow_call"]["inputs"]["use-database-mirrors"];
    ensure!(
        input["type"] == "boolean" && input["default"] == true,
        "Trivy must prefer TARS Cloud mirrors by default with a boolean opt-out"
    );
    ensure!(
        super::step(&workflow, "scan", "configuration")?["env"]["USE_DATABASE_MIRRORS"]
            == "${{ inputs.use-database-mirrors }}",
        "mirror input must pass through an environment variable before shell entry"
    );
    Ok(())
}

pub(super) fn scripts(root: &Path) -> Result<()> {
    let workflow = super::load(root, ".github/workflows/consumer-trivy.yaml")?;
    for enabled in [None, Some("true"), Some("false")] {
        let encoded = if enabled == Some("false") {
            String::new()
        } else {
            ARGS.join("\0") + "\0"
        };
        let mut case = json!({
            "tools":["realpath"],
            "env":{"PROJECT_DIRECTORY":"project","CONFIG_FILE":"auto","SCAN_PATH":".","RUNNER_TEMP":"${state}/tmp"},
            "files":{"project/trivy.yaml":"severity:\n  - MEDIUM\n", "reports/report/placeholder":""},
            "commands":{"mktemp":[{"args":["-d","${state}/tmp/trivy-XXXXXXXX"],"stdout":"${workspace}/reports/report\n","exit":0}]},
            "expect":{"exit":0,"calls":[{"command":"mktemp","args":["-d","${state}/tmp/trivy-XXXXXXXX"]}],
                "files":{"reports/report/database-args":encoded,"project/trivy.yaml":"severity:\n  - MEDIUM\n"}}
        });
        if let Some(enabled) = enabled {
            case["env"]["USE_DATABASE_MIRRORS"] = json!(enabled);
        }
        super::execute(root, &workflow, "scan", "configuration", case)?;
    }
    for enabled in [true, false] {
        for gate in [false, true] {
            scan(root, &workflow, enabled, gate)?;
        }
    }
    for id in ["scan", "evaluate_gate"] {
        super::execute(
            root,
            &workflow,
            "scan",
            id,
            json!({
                "env":{"SARIF_FILE":"report/results.sarif","GATE_REPORT":"report/gate.json"},
                "commands":{"trivy":[]}, "expect":{"exit":1,"calls":[]}
            }),
        )?;
    }
    Ok(())
}

fn scan(root: &Path, workflow: &serde_json::Value, enabled: bool, gate: bool) -> Result<()> {
    let repositories = if enabled { ARGS.to_vec() } else { vec![] };
    let encoded = if enabled {
        ARGS.join("\0") + "\0"
    } else {
        String::new()
    };
    let id = if gate { "evaluate_gate" } else { "scan" };
    let mut args = vec!["filesystem", "--config", "policy with spaces.yaml"];
    if !gate {
        args.extend(&repositories);
    }
    args.extend([
        "--cache-dir",
        "cache",
        "--format",
        if gate { "json" } else { "sarif" },
        "--output",
        if gate {
            "report with spaces/gate.json"
        } else {
            "report with spaces/results.sarif"
        },
        "--exit-code",
        if gate { "10" } else { "0" },
    ]);
    if gate {
        args.extend(&repositories);
    }
    args.extend(["--", "target with spaces"]);
    let case = json!({
        "env":{"SCAN_CONFIG":"policy with spaces.yaml","GATE_CONFIG":"policy with spaces.yaml","FAIL_ON_FINDINGS":"true",
            "SARIF_FILE":"report with spaces/results.sarif","GATE_REPORT":"report with spaces/gate.json",
            "SCAN_TARGET":"target with spaces","TRIVY_CACHE_DIR":"cache","DEVENV_RESULT_FILE":"result.json"},
        "files":{"report with spaces/database-args":encoded,"policy with spaces.yaml":"severity:\n  - MEDIUM\n"},
        "commands":{"trivy":[{"args":args,"exit":0}]},
        "expect":{"exit":0,"calls":[{"command":"trivy","args":args}],"files":{"policy with spaces.yaml":"severity:\n  - MEDIUM\n"}}
    });
    super::execute(root, workflow, "scan", id, case.clone())?;
    let mut clean = case;
    clean["env"]["DEVENV_PROFILE"] = json!("${workspace}/profile");
    clean["commands"]["ambient-trivy"] = json!([{"args":[],"exit":99}]);
    clean["command-paths"] =
        json!({"trivy":["${workspace}/profile/bin/trivy"],"ambient-trivy":["${bin}/trivy"]});
    super::execute(root, workflow, "scan", id, clean)
        .context("mirror selection in clean consumer profile")
}
