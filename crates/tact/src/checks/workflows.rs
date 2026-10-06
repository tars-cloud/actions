use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

mod codeql_platform;

pub(super) fn load(root: &Path, name: &str) -> Result<Value> {
    Ok(serde_norway::from_str(&fs::read_to_string(
        root.join(name),
    )?)?)
}

pub(super) fn step<'a>(workflow: &'a Value, job: &str, id: &str) -> Result<&'a Value> {
    workflow["jobs"][job]["steps"]
        .as_array()
        .context("workflow steps")?
        .iter()
        .find(|step| step["id"] == id)
        .with_context(|| format!("missing workflow step {job}/{id}"))
}

pub(super) fn inputs(call: &Value, definitions: &Value) -> Result<()> {
    for field in ["with", "secrets"] {
        let kind = if field == "with" { "inputs" } else { "secrets" };
        if let Some(values) = call[field].as_object() {
            for (key, value) in values {
                let definition = definitions[kind]
                    .get(key)
                    .with_context(|| format!("unknown {kind} {key}"))?;
                if kind == "inputs" && !value.as_str().is_some_and(|s| s.contains("${{")) {
                    ensure!(
                        match definition["type"].as_str() {
                            Some("boolean") => value.is_boolean(),
                            Some("number") => value.is_number(),
                            _ => value.is_string(),
                        },
                        "wrong input type: {key}"
                    );
                }
            }
        }
        if let Some(values) = definitions[kind].as_object() {
            for (key, value) in values {
                ensure!(
                    value["required"] != true
                        || value.get("default").is_some()
                        || call[field].get(key).is_some(),
                    "missing required {kind}: {key}"
                );
            }
        }
    }
    Ok(())
}

pub(super) fn contracts(root: &Path) -> Result<()> {
    super::consumer_setup::contracts(root)?;
    super::cargo_crap::contracts(root)?;
    for name in [
        "consumer-devenv-update",
        "consumer-devenv-ci",
        "consumer-trivy",
        "consumer-codeql",
        "consumer-cargo-crap",
        "consumer-rust-release-candidate",
        "consumer-rust-release-prepare",
        "consumer-rust-release-publish",
    ] {
        let workflow = load(root, &format!(".github/workflows/{name}.yaml"))?;
        ensure!(
            workflow["on"]["workflow_call"].is_object(),
            "missing workflow_call"
        );
        for job in workflow["jobs"]
            .as_object()
            .context("workflow jobs")?
            .values()
        {
            ensure!(
                job["runs-on"]
                    .as_str()
                    .is_some_and(|runner| runner.contains("inputs.runs-on")),
                "every consumer job must honor runner selection: {name}"
            );
            for step in job["steps"].as_array().context("steps")? {
                ensure!(
                    step["id"].is_string() && step["name"].is_string(),
                    "step identity"
                );
                if let Some(reference) = step["uses"].as_str() {
                    if let Some(path) = reference.strip_prefix("$/") {
                        let metadata = load(root, &format!("{path}/action.yaml"))?;
                        if let Some(values) = step["with"].as_object() {
                            for key in values.keys() {
                                ensure!(
                                    metadata["inputs"].get(key).is_some(),
                                    "unknown composite input {path}/{key}"
                                );
                            }
                        }
                    } else {
                        ensure!(
                            super::metadata::pinned_upstream(reference),
                            "expected an allowed workflow action pinned to a full SHA: {reference}"
                        );
                    }
                }
                for run in [step["run"].as_str(), step["with"]["run"].as_str()]
                    .into_iter()
                    .flatten()
                {
                    ensure!(
                        !run.contains("${{"),
                        "workflow inputs must pass through environment variables"
                    );
                }
            }
        }
        let example = load(root, &format!("workflows/{name}/example.yaml"))?;
        let mut found = false;
        for call in example["jobs"]
            .as_object()
            .context("example jobs")?
            .values()
        {
            let Some(reference) = call["uses"].as_str() else {
                ensure!(
                    call["steps"].is_array(),
                    "example must define steps or call a reusable workflow"
                );
                continue;
            };
            let (path, revision) = reference
                .strip_prefix("tars-cloud/actions/")
                .context("example owner")?
                .split_once('@')
                .context("example revision")?;
            ensure!(
                revision == "v3"
                    || (revision.len() == 40 && revision.chars().all(|c| c.is_ascii_hexdigit())),
                "example needs the next release alias or full SHA"
            );
            let target = load(root, path)?;
            inputs(call, &target["on"]["workflow_call"])?;
            found |= path == format!(".github/workflows/{name}.yaml");
        }
        ensure!(found, "example must exercise its reusable workflow");
    }
    super::devenv_update::contracts(root)?;
    trivy_contract(root)?;
    codeql_contract(root)
}

fn trivy_contract(root: &Path) -> Result<()> {
    let trivy = load(root, ".github/workflows/consumer-trivy.yaml")?;
    let timeout = &trivy["on"]["workflow_call"]["inputs"]["timeout-minutes"];
    ensure!(
        timeout["type"] == "number" && timeout["default"] == 30,
        "Trivy timeout must remain an optional numeric input with a 30-minute default"
    );
    ensure!(
        trivy["jobs"]["scan"]["timeout-minutes"] == "${{ inputs.timeout-minutes }}",
        "Trivy job must honor the caller's timeout"
    );
    for (id, action) in [
        ("cache", "setup-cache"),
        ("devenv", "setup-devenv"),
        ("trivy", "setup-trivy"),
        ("scan", "run-devenv"),
        ("report", "report-status"),
    ] {
        let s = step(&trivy, "scan", id)?;
        ensure!(
            s["uses"] == format!("$/composite/{action}"),
            "same-revision {action} composition"
        );
        if id != "report" {
            for (input, expected) in [
                ("type", "${{ inputs.type }}"),
                ("working-directory", "${{ inputs.working-directory }}"),
                ("flake-shell", "${{ inputs.flake-shell }}"),
                (
                    "system",
                    "${{ inputs.system || (inputs.runner-architecture == 'ARM64' && 'aarch64-linux') || '' }}",
                ),
            ] {
                ensure!(
                    s["with"][input] == expected,
                    "consistent consumer environment: {id}/{input}"
                );
            }
        }
    }
    let cache = step(&trivy, "scan", "cache")?;
    ensure!(cache["with"]["tools"] == "trivy", "only cache Trivy");
    for input in [
        "s3-endpoint",
        "s3-bucket",
        "s3-region",
        "s3-access-key",
        "s3-secret-key",
        "s3-session-token",
    ] {
        ensure!(
            cache["with"][input]
                .as_str()
                .is_some_and(|v| v.contains("steps.checkout.outputs.trusted == 'true'")),
            "cache credentials require trust"
        );
    }
    let upload = step(&trivy, "scan", "upload")?;
    ensure!(
        upload["if"]
            == "always() && inputs.upload-sarif && steps.sarif.outputs.available == 'true'",
        "upload existing reports after failed scans"
    );
    ensure!(
        step(&trivy, "scan", "report")?["with"]["results"]
            .as_str()
            .is_some_and(|s| s.contains("devenv=") && s.contains("upload=")),
        "report infrastructure failures"
    );
    ensure!(
        trivy["on"]["workflow_call"]["inputs"]["gate-config-file"]["default"] == "",
        "separate gating must remain opt-in"
    );
    ensure!(
        step(&trivy, "scan", "evaluate_gate")?["if"]
            .as_str()
            .is_some_and(|value| value.contains("steps.scan.outcome == 'success'")),
        "gate analysis requires a completed reporting scan"
    );
    let steps = trivy["jobs"]["scan"]["steps"]
        .as_array()
        .context("Trivy steps")?;
    let index = |id| {
        steps
            .iter()
            .position(|step| step["id"] == id)
            .context("Trivy step ID")
    };
    ensure!(
        index("upload")? < index("enforce_gate")? && index("cleanup")? < index("report")?,
        "report upload and cleanup must precede aggregate failure"
    );
    Ok(())
}

fn codeql_contract(root: &Path) -> Result<()> {
    let codeql = load(root, ".github/workflows/consumer-codeql.yaml")?;
    let timeout = &codeql["on"]["workflow_call"]["inputs"]["timeout-minutes"];
    ensure!(
        timeout["type"] == "number" && timeout["default"] == 180,
        "CodeQL timeout must remain an optional numeric input with a 180-minute default"
    );
    ensure!(
        codeql["jobs"]["analyze"]["timeout-minutes"] == "${{ inputs.timeout-minutes }}",
        "CodeQL language jobs must honor the caller's timeout"
    );
    ensure!(
        codeql["jobs"]["status"]["timeout-minutes"] == 5,
        "CodeQL summary must retain its five-minute timeout"
    );
    ensure!(
        codeql["on"]["workflow_call"]["inputs"]["job-name"]["default"] == "CodeQL"
            && codeql["jobs"]["analyze"]["name"]
                .as_str()
                .is_some_and(|name| name.starts_with("${{ inputs.job-name }} - "))
            && codeql["jobs"]["status"]["name"] == "${{ inputs.job-name }} - Summary",
        "preserve default CodeQL names and label each caller's child jobs"
    );
    ensure!(
        codeql["on"]["workflow_call"]["inputs"]["type"]["default"] == "runner",
        "preserve existing CodeQL callers"
    );
    for (id, action) in [
        ("devenv", "setup-devenv"),
        ("setup_environment", "run-devenv"),
        ("environment", "run-devenv"),
        ("build_environment", "run-devenv"),
    ] {
        let s = step(&codeql, "analyze", id)?;
        ensure!(
            s["uses"] == format!("$/composite/{action}"),
            "CodeQL must compose {action} at its own revision"
        );
        ensure!(
            s["with"]["type"] == "${{ matrix.type || inputs.type }}",
            "matrix environment selection"
        );
        ensure!(
            s["if"]
                .as_str()
                .is_some_and(|v| v.contains("(matrix.type || inputs.type) != 'runner'")),
            "runner mode must skip Nix setup"
        );
        for input in ["working-directory", "flake-shell"] {
            ensure!(
                s["with"][input] == format!("${{{{ inputs.{input} }}}}"),
                "consistent CodeQL environment: {id}/{input}"
            );
        }
    }
    let steps = codeql["jobs"]["analyze"]["steps"]
        .as_array()
        .context("CodeQL steps")?;
    let position = |id| steps.iter().position(|s| s["id"] == id).unwrap();
    ensure!(
        position("capture_host_tools") < position("devenv")
            && position("capture_host_tools") < position("setup_environment")
            && position("devenv") < position("environment")
            && position("environment") < position("toolchain")
            && position("toolchain") < position("init"),
        "activate the consumer toolchain before CodeQL checks and initialization"
    );
    ensure!(
        codeql["jobs"]["analyze"]["strategy"]["fail-fast"] == false,
        "analyze every language"
    );
    ensure!(
        step(&codeql, "analyze", "init")?["with"]["config-file"]
            == "${{ steps.configuration.outputs.config }}",
        "consumer CodeQL config"
    );
    ensure!(
        codeql["jobs"]["status"]["needs"] == "analyze"
            && codeql["jobs"]["status"]["if"] == "always()",
        "report failed matrix jobs"
    );
    let ci = load(root, ".github/workflows/test-security-workflows.yaml")?;
    for (job, name) in [
        ("codeql", "CodeQL - Devenv - ${{ matrix.architecture }}"),
        (
            "codeql-flakes",
            "CodeQL - Flake - ${{ matrix.architecture }}",
        ),
        ("codeql-runner", "CodeQL - Runner Compatibility"),
    ] {
        ensure!(
            ci["jobs"][job]["with"]["job-name"] == name,
            "distinguish CodeQL child jobs: {job}"
        );
    }
    for job in ci["jobs"].as_object().context("security CI")?.values() {
        let path = job["uses"]
            .as_str()
            .context("CI reusable call")?
            .strip_prefix("./")
            .context("CI must use revision under test")?;
        inputs(job, &load(root, path)?["on"]["workflow_call"])?;
    }
    Ok(())
}

pub(super) fn execute(
    root: &Path,
    workflow: &Value,
    job: &str,
    id: &str,
    mut case: Value,
) -> Result<()> {
    let step = step(workflow, job, id)?;
    let script = step["run"]
        .as_str()
        .or_else(|| step["with"]["run"].as_str())
        .context("production script")?;
    case["id"] = json!(id);
    case["description"] =
        json!("Execute the production workflow script in an isolated Tact fixture");
    case["command"] = json!(["bash", "-euo", "pipefail", "-c", script]);
    let manifest = serde_json::from_value(json!({"version":1,"tests":[]}))?;
    crate::runner::run(root, &manifest, &serde_json::from_value(case)?)
        .with_context(|| format!("{job}/{id}"))
}

pub(super) fn run(root: &Path) -> Result<()> {
    super::consumer_ci::run(root)?;
    super::devenv_update::run(root)?;
    environment_activation(root)?;
    contracts(root)?;
    rust_releases(root)?;
    codeql_scripts(root)?;
    trivy_scripts(root)?;
    println!(
        "PASS reusable workflow scripts: language modes, config discovery, trust, scan arguments, failures and cleanup"
    );
    Ok(())
}

fn codeql_scripts(root: &Path) -> Result<()> {
    let codeql = load(root, ".github/workflows/consumer-codeql.yaml")?;
    let base = json!({
        "env":{"RUNNER_OS":"Linux","LANGUAGE":"actions","BUILD_MODE":"","BUILD_COMMAND":"","CONFIG_FILE":"auto","ENVIRONMENT_TYPE":"runner"},
        "expect":{"exit":0,"calls":[],"github-output":{"mode":"none","config":""}}
    });
    for (language, mode) in [
        ("actions", "none"),
        ("javascript-typescript", "none"),
        ("python", "none"),
        ("ruby", "none"),
        ("rust", "none"),
        ("c-cpp", "none"),
        ("csharp", "none"),
        ("java-kotlin", "autobuild"),
        ("go", "autobuild"),
        ("swift", "autobuild"),
    ] {
        let mut case = base.clone();
        case["env"]["LANGUAGE"] = json!(language);
        if language == "swift" {
            case["env"]["RUNNER_OS"] = json!("macOS");
        }
        case["expect"]["github-output"]["mode"] = json!(mode);
        execute(root, &codeql, "analyze", "configuration", case)?;
    }
    for (key, value) in [
        ("LANGUAGE", "unknown"),
        ("LANGUAGE", "$(touch injected)"),
        ("LANGUAGE", "swift"),
        ("BUILD_MODE", "manual"),
        ("BUILD_COMMAND", "echo ignored"),
        ("CONFIG_FILE", "missing.yml"),
        ("CONFIG_FILE", "../outside"),
        ("CONFIG_FILE", "/absolute"),
        ("CONFIG_FILE", "name\ninjected=true"),
        ("RUNNER_OS", "Windows"),
        ("ENVIRONMENT_TYPE", "unknown"),
    ] {
        let mut case = base.clone();
        case["env"][key] = json!(value);
        case["expect"] = json!({"exit":1,"calls":[],"files":{"injected":null}});
        execute(root, &codeql, "analyze", "configuration", case)?;
    }
    for environment_type in ["devenv", "flakes"] {
        let mut case = base.clone();
        case["env"]["ENVIRONMENT_TYPE"] = json!(environment_type);
        execute(root, &codeql, "analyze", "configuration", case.clone())?;
        case["env"]["RUNNER_OS"] = json!("macOS");
        case["expect"] = json!({"exit":1,"calls":[]});
        execute(root, &codeql, "analyze", "configuration", case)?;
    }
    for file in [".github/codeql-config.yml", "config with spaces.yml"] {
        let mut case = base.clone();
        case["files"] = json!({file:"---\npaths-ignore: [tests]\n"});
        if !file.starts_with(".github") {
            case["env"]["CONFIG_FILE"] = json!(file);
        }
        case["expect"]["github-output"]["config"] = json!(file);
        execute(root, &codeql, "analyze", "configuration", case)?;
    }
    for mode in ["autobuild", "manual"] {
        let mut case = base.clone();
        case["env"]["LANGUAGE"] = json!("c-cpp");
        case["env"]["BUILD_MODE"] = json!(mode);
        if mode == "manual" {
            case["env"]["BUILD_COMMAND"] = json!("make");
        }
        case["expect"]["github-output"]["mode"] = json!(mode);
        execute(root, &codeql, "analyze", "configuration", case)?;
    }
    for id in ["setup", "build", "setup_environment", "build_environment"] {
        let key = if id.starts_with("setup") {
            "SETUP_COMMAND"
        } else {
            "BUILD_COMMAND"
        };
        execute(
            root,
            &codeql,
            "analyze",
            id,
            json!({"env":{key:"printf 'literal $(command)' > result"},"expect":{"exit":0,"calls":[],"files":{"result":"literal $(command)"}}}),
        )?;
        execute(
            root,
            &codeql,
            "analyze",
            id,
            json!({"env":{key:"exit 7"},"expect":{"exit":7,"calls":[]}}),
        )?;
    }
    execute(
        root,
        &codeql,
        "analyze",
        "toolchain",
        json!({"env":{"ENVIRONMENT_TYPE":"runner"},"expect":{"exit":1,"calls":[]}}),
    )?;
    execute(
        root,
        &codeql,
        "analyze",
        "toolchain",
        json!({"commands":{"cargo":[]},"expect":{"exit":1,"calls":[]}}),
    )?;
    execute(
        root,
        &codeql,
        "analyze",
        "toolchain",
        json!({"commands":{"cargo":[],"rustc":[]},"expect":{"exit":0,"calls":[]}}),
    )?;

    Ok(())
}

fn trivy_scripts(root: &Path) -> Result<()> {
    let trivy = load(root, ".github/workflows/consumer-trivy.yaml")?;
    let base = json!({
        "tools":["realpath","mktemp"],
        "env":{"PROJECT_DIRECTORY":"project","CONFIG_FILE":"auto","SCAN_PATH":".","REPOSITORY":"example/consumer","HEAD_REPOSITORY":"","ACTOR":"developer","PR_AUTHOR":"","RUNNER_TEMP":"${state}/tmp"},
        "files":{"project/input":"fixture"},
        "expect":{"exit":0,"calls":[]}
    });
    for separate_gate in [false, true] {
        let mut case = base.clone();
        case["tools"] = json!(["realpath"]);
        case["commands"] = json!({"mktemp":[{"args":["-d","${state}/tmp/trivy-XXXXXXXX"],"stdout":"${state}/tmp/report\n","exit":0}]});
        case["files"]["project/trivy.yaml"] = json!("scan: {scanners: [secret]}");
        let gate = if separate_gate {
            "${workspace}/project/gate.yaml"
        } else {
            ""
        };
        if separate_gate {
            case["env"]["GATE_CONFIG_FILE"] = json!("gate.yaml");
            case["files"]["project/gate.yaml"] = json!("scan: {scanners: [secret]}");
        }
        case["expect"] = json!({"exit":0,"calls":[{"command":"mktemp","args":["-d","${state}/tmp/trivy-XXXXXXXX"]}],"github-output":{"config":"${workspace}/project/trivy.yaml","gate-config":gate,"gate-report":"${state}/tmp/report/gate.json","target":"${workspace}/project","sarif":"${state}/tmp/report/results.sarif"}});
        execute(root, &trivy, "scan", "configuration", case)?;
    }
    for (key, value) in [
        ("CONFIG_FILE", "missing.yaml"),
        ("CONFIG_FILE", "../outside"),
        ("SCAN_PATH", ".."),
        ("PROJECT_DIRECTORY", ".."),
        ("SCAN_PATH", "name\ninjected=true"),
        ("GATE_CONFIG_FILE", "missing.yaml"),
        ("GATE_CONFIG_FILE", "../outside"),
        ("GATE_CONFIG_FILE", "name\ninjected=true"),
    ] {
        let mut case = base.clone();
        case["env"][key] = json!(value);
        case["files"]["outside"] = json!("fixture");
        case["expect"]["exit"] = json!(1);
        execute(root, &trivy, "scan", "configuration", case)?;
    }
    gate_scripts(root, &trivy)?;
    for (config, gate, separate, exit) in [
        ("", "false", "", 0),
        ("config with spaces.yaml", "true", "", 0),
        ("literal $(touch injected).yaml", "true", "", 1),
        ("", "false", "", 2),
        ("report.yaml", "true", "gate.yaml", 0),
    ] {
        let args = json!([
            "filesystem",
            "--config",
            if config.is_empty() {
                "/dev/null"
            } else {
                config
            },
            "--cache-dir",
            "cache with spaces",
            "--format",
            "sarif",
            "--output",
            "report.sarif",
            "--exit-code",
            if gate == "true" && separate.is_empty() {
                "1"
            } else {
                "0"
            },
            "--",
            "target with spaces"
        ]);
        let case = json!({
            "env":{"SCAN_CONFIG":config,"FAIL_ON_FINDINGS":gate,"GATE_CONFIG":separate,"TRIVY_CACHE_DIR":"cache with spaces","SARIF_FILE":"report.sarif","SCAN_TARGET":"target with spaces"},
            "commands":{"trivy":[{"args":args,"exit":exit}]},
            "expect":{"exit":exit,"calls":[{"command":"trivy","args":args}],"files":{"injected":null}}
        });
        execute(root, &trivy, "scan", "scan", case.clone())?;
        let mut clean = case;
        clean["env"]["DEVENV_PROFILE"] = json!("${workspace}/profile");
        clean["commands"]["ambient-trivy"] = json!([{"args":[],"exit":99}]);
        clean["command-paths"] =
            json!({"trivy":["${workspace}/profile/bin/trivy"],"ambient-trivy":["${bin}/trivy"]});
        execute(root, &trivy, "scan", "scan", clean)?;
    }
    for (files, available) in [
        (json!({}), "false"),
        (json!({"report.sarif":""}), "false"),
        (json!({"report.sarif":"{}"}), "true"),
    ] {
        execute(
            root,
            &trivy,
            "scan",
            "sarif",
            json!({"env":{"SARIF_FILE":"report.sarif"},"files":files,"expect":{"exit":0,"calls":[],"github-output":{"available":available}}}),
        )?;
    }
    execute(
        root,
        &trivy,
        "scan",
        "cleanup",
        json!({"tools":["rm","rmdir"],"env":{"SARIF_FILE":"report/results.sarif"},"files":{"report/results.sarif":"{}","keep":"retained"},"expect":{"exit":0,"calls":[],"files":{"report/results.sarif":null,"keep":"retained"}}}),
    )?;
    Ok(())
}

fn gate_scripts(root: &Path, trivy: &Value) -> Result<()> {
    let args = json!([
        "filesystem",
        "--config",
        "gate with spaces.yaml",
        "--cache-dir",
        "cache",
        "--format",
        "json",
        "--output",
        "gate.json",
        "--exit-code",
        "10",
        "--",
        "target"
    ]);
    for status in [0, 10, 1, 2, 64] {
        let findings = status == 10;
        let completed = matches!(status, 0 | 10);
        let mut expected = json!({"exit":if completed {0} else {status},"calls":[{"command":"trivy","args":args}]});
        if completed {
            expected["files"] = json!({"result.json":format!("{{\"findings\":{findings}}}\n")});
        } else {
            expected["files"] = json!({"result.json":null});
        }
        execute(
            root,
            trivy,
            "scan",
            "evaluate_gate",
            json!({
                "env":{"GATE_CONFIG":"gate with spaces.yaml","GATE_REPORT":"gate.json","TRIVY_CACHE_DIR":"cache","SCAN_TARGET":"target","DEVENV_RESULT_FILE":"result.json"},
                "commands":{"trivy":[{"args":args,"exit":status}]},"expect":expected
            }),
        )?;
    }
    for (findings, fail, status) in [
        ("true", "true", 1),
        ("true", "false", 0),
        ("false", "true", 0),
    ] {
        execute(
            root,
            trivy,
            "scan",
            "enforce_gate",
            json!({"env":{"FINDINGS":findings,"FAIL_ON_FINDINGS":fail},"expect":{"exit":status,"calls":[]}}),
        )?;
    }
    execute(
        root,
        trivy,
        "scan",
        "cleanup",
        json!({"tools":["rm","rmdir"],"env":{"SARIF_FILE":"report/results.sarif","GATE_REPORT":"report/gate.json"},"files":{"report/results.sarif":"{}","report/gate.json":"{}","keep":"retained"},"expect":{"exit":0,"calls":[],"files":{"report/results.sarif":null,"report/gate.json":null,"keep":"retained"}}}),
    )?;
    Ok(())
}

fn rust_releases(root: &Path) -> Result<()> {
    let sha = "a".repeat(40);
    for operation in ["candidate", "prepare", "publish", "composite"] {
        let workflow = if operation == "composite" {
            let action = load(root, "composite/release-rust/action.yaml")?;
            json!({"jobs":{"release":{"steps":action["runs"]["steps"]}}})
        } else {
            let workflow = load(
                root,
                &format!(".github/workflows/consumer-rust-release-{operation}.yaml"),
            )?;
            let checkout = step(&workflow, "release", "checkout")?;
            ensure!(
                checkout["with"]["ref"] == "${{ inputs.commit-sha }}"
                    && checkout["with"]["fetch-depth"] == 0
                    && checkout["uses"] == "$/composite/setup-consumer/scripts/checkout",
                "release checkout must use the exact commit and full history without saved credentials"
            );
            let release = step(&workflow, "release", "release")?;
            ensure!(
                release["uses"] == "$/composite/release-rust"
                    && release["with"]["command"] == operation,
                "release workflows must compose their own revision"
            );
            ensure!(
                workflow.get("concurrency").is_none()
                    && workflow["jobs"]["release"].get("concurrency").is_none(),
                "callee must not reacquire the caller's release queue"
            );
            ensure!(
                workflow["permissions"]["contents"]
                    == if operation == "publish" {
                        "write"
                    } else {
                        "read"
                    },
                "only publication uses a writable workflow token"
            );
            workflow
        };
        ensure!(
            workflow["jobs"]["release"]["steps"][0]["id"] == "trigger",
            "reject untrusted release contexts before checkout, credentials or consumer code"
        );
        let base = json!({
            "env":{"RELEASE_COMMIT":sha,"GITHUB_SHA":sha,"GITHUB_REF":"refs/heads/main","DEFAULT_BRANCH":"main","GITHUB_EVENT_NAME":"push"},
            "expect":{"exit":0,"calls":[]}
        });
        for event in ["push", "workflow_dispatch"] {
            let mut case = base.clone();
            case["env"]["GITHUB_EVENT_NAME"] = json!(event);
            execute(root, &workflow, "release", "trigger", case)?;
        }
        for (key, value) in [
            ("RELEASE_COMMIT", "main"),
            ("RELEASE_COMMIT", "$(touch injected)"),
            ("GITHUB_SHA", "b".repeat(40).as_str()),
            ("GITHUB_REF", "refs/heads/topic"),
            ("GITHUB_EVENT_NAME", "pull_request"),
            ("GITHUB_EVENT_NAME", "pull_request_target"),
            ("GITHUB_EVENT_NAME", "workflow_run"),
        ] {
            let mut case = base.clone();
            case["env"][key] = json!(value);
            case["expect"] = json!({"exit":1,"calls":[],"files":{"injected":null}});
            execute(root, &workflow, "release", "trigger", case)?;
        }
    }
    println!(
        "PASS Rust release workflows: exact checkout, permissions, composition and trigger boundaries"
    );
    Ok(())
}

fn environment_activation(root: &Path) -> Result<()> {
    let workflow = load(root, ".github/workflows/consumer-codeql.yaml")?;
    let activate = step(&workflow, "analyze", "environment")?["with"]["run"]
        .as_str()
        .context("CodeQL environment activation script")?;
    let validate = step(&workflow, "analyze", "toolchain")?["run"]
        .as_str()
        .context("CodeQL toolchain validation script")?;
    let scratch = root.join(".tars/scratch/codeql");
    fs::create_dir_all(&scratch)?;
    let fixture = tempfile::tempdir_in(scratch)?;
    let tools = fixture.path().join("consumer tools");
    let runner = fixture.path().join("runner tools");
    let boundary = fixture.path().join("boundary");
    let bridge = prepare_host_tools(fixture.path())?;
    let bash = prepare_activation_tools(&tools, &runner)?;
    let env_file = fixture.path().join("env");
    let path_file = fixture.path().join("path");
    let invoke = |script: &str| {
        let mut command = Command::new(&bash);
        command
            .args(["--noprofile", "--norc", "-euo", "pipefail", "-c", script])
            .env_clear()
            .env("PATH", &runner)
            .env("ENVIRONMENT_TYPE", "devenv")
            .env("CODEQL_PATH_BOUNDARY", &boundary)
            .env("RUNNER_TEMP", fixture.path())
            .env("CODEQL_LANGUAGE", "rust")
            .env("GITHUB_ENV", &env_file)
            .env("GITHUB_PATH", &path_file);
        command
    };
    let missing = invoke(validate).output()?;
    ensure!(!missing.status.success(), "runner unexpectedly has Cargo");
    ensure!(
        String::from_utf8_lossy(&missing.stdout).contains("cargo"),
        "wrong missing-tool failure"
    );
    let path = std::env::join_paths([&tools, &boundary, &runner])?;
    let marker = "literal $(touch injected)\nsecond line";
    let activate_result = invoke(activate)
        .env("PATH", &path)
        .env("CODEQL_EXPORT_VARIABLES", "CUSTOM_TOOL_CONFIG value")
        .env("RUST_SRC_PATH", "source with spaces")
        .env("NIX_CFLAGS_COMPILE", "-Iinclude")
        .env("CUSTOM_TOOL_CONFIG", marker)
        .env("value", "lowercase setting")
        .env("NIX_CONFIG", "extra-access-tokens = github.com=secret")
        .env("UNRELATED_SECRET", "not-for-export")
        .env("GITHUB_TOKEN", "not-for-export")
        .output()?;
    ensure!(
        activate_result.status.success(),
        "activation: {activate_result:?}"
    );
    ensure!(
        activate_result.stdout.is_empty() && activate_result.stderr.is_empty(),
        "activation logged environment values"
    );
    let exported = crate::output::values(&fs::read_to_string(&env_file)?)?;
    ensure!(
        exported.len() == 4,
        "unexpected environment export: {:?}",
        exported.keys()
    );
    let paths = fs::read_to_string(&path_file)?;
    let restored_path = std::env::join_paths(paths.lines().rev())?;
    let expected_path = std::env::join_paths([&bridge, &tools, &boundary, &runner])?;
    ensure!(
        restored_path == expected_path,
        "environment PATH order changed"
    );
    let restored = invoke(&format!("{validate}\ncargo"))
        .envs(exported)
        .env("PATH", restored_path)
        .output()?;
    ensure!(restored.status.success(), "CodeQL subprocess: {restored:?}");
    ensure!(
        String::from_utf8(restored.stdout)?
            == format!("source with spaces|-Iinclude|{marker}|lowercase setting"),
        "subprocess lost consumer toolchain configuration"
    );
    reject_unsafe_exports(activate, &path, &env_file, &path_file, &invoke)?;
    for tool in ["cargo", "rustc"] {
        fs::copy(tools.join(tool), runner.join(tool))?;
    }
    let ambient = invoke(activate)
        .env("PATH", std::env::join_paths([&boundary, &runner])?)
        .env("CODEQL_EXPORT_VARIABLES", "")
        .output()?;
    ensure!(
        !ambient.status.success(),
        "runner toolchain substituted for missing consumer packages"
    );
    clean_profile(
        fixture.path(),
        &tools,
        &runner,
        &path_file,
        activate,
        &invoke,
        &bash,
    )?;
    let cpp = invoke(activate)
        .env("PATH", &path)
        .env("CODEQL_LANGUAGE", "c-cpp")
        .env("CODEQL_EXPORT_VARIABLES", "")
        .output()?;
    ensure!(cpp.status.success(), "C/C++ activation: {cpp:?}");
    let exported = crate::output::values(&fs::read_to_string(&env_file)?)?;
    ensure!(
        exported["CODEQL_EXTRACTOR_CPP_AUTOINSTALL_DEPENDENCIES"] == "false",
        "Nix environments must not install undeclared C/C++ build tools"
    );
    codeql_platform::run(root)
}

fn prepare_host_tools(root: &Path) -> Result<std::path::PathBuf> {
    let bridge = root.join("tars-codeql-host-tools");
    fs::create_dir_all(&bridge)?;
    std::os::unix::fs::symlink(crate::runner::executable("uname")?, bridge.join("uname"))?;
    Ok(bridge)
}

fn reject_unsafe_exports(
    activate: &str,
    path: &std::ffi::OsStr,
    env_file: &Path,
    path_file: &Path,
    invoke: &impl Fn(&str) -> Command,
) -> Result<()> {
    for invalid in [
        "GITHUB_TOKEN",
        "NIX_CONFIG",
        "NODE_OPTIONS",
        "TARS_CODEQL_NAME",
        "TARS_CLOUD_CODEQL_HOST_TOOLS",
        "BAD-NAME",
        "PATH",
        "$(touch injected)",
    ] {
        fs::write(env_file, "")?;
        fs::write(path_file, "")?;
        let result = invoke(activate)
            .env("PATH", path)
            .env("CODEQL_EXPORT_VARIABLES", invalid)
            .output()?;
        ensure!(
            !result.status.success(),
            "unsafe environment name accepted: {invalid}"
        );
        ensure!(
            fs::read_to_string(env_file)?.is_empty() && fs::read_to_string(path_file)?.is_empty(),
            "partial export after invalid input"
        );
    }
    Ok(())
}

fn prepare_activation_tools(tools: &Path, runner: &Path) -> Result<std::path::PathBuf> {
    fs::create_dir_all(tools)?;
    fs::create_dir_all(runner)?;
    let bash = crate::runner::executable("bash")?;
    for tool in ["cargo", "rustc"] {
        let file = tools.join(tool);
        fs::write(
            &file,
            format!(
                "#!{}\nprintf '%s' \"$RUST_SRC_PATH|$NIX_CFLAGS_COMPILE|$CUSTOM_TOOL_CONFIG|$value\"\n",
                bash.display()
            ),
        )?;
        fs::set_permissions(file, fs::Permissions::from_mode(0o700))?;
    }
    Ok(bash)
}

fn clean_profile(
    root: &Path,
    tools: &Path,
    runner: &Path,
    path_file: &Path,
    activate: &str,
    invoke: &impl Fn(&str) -> Command,
    bash: &Path,
) -> Result<()> {
    let profile = root.join("profile");
    fs::create_dir_all(&profile)?;
    std::os::unix::fs::symlink(tools, profile.join("bin"))?;
    fs::write(
        runner.join("cargo"),
        format!("#!{}\nexit 99\n", bash.display()),
    )?;
    fs::write(path_file, "")?;
    let clean = invoke(activate)
        .env_remove("CODEQL_PATH_BOUNDARY")
        .env("DEVENV_PROFILE", &profile)
        .env("PATH", std::env::join_paths([&runner, &tools])?)
        .env("CODEQL_EXPORT_VARIABLES", "")
        .output()?;
    ensure!(clean.status.success(), "clean devenv activation: {clean:?}");
    let clean_paths = fs::read_to_string(path_file)?;
    let clean_tool = invoke("cargo")
        .env("PATH", std::env::join_paths(clean_paths.lines().rev())?)
        .env("RUST_SRC_PATH", "declared-profile")
        .env("NIX_CFLAGS_COMPILE", "")
        .env("CUSTOM_TOOL_CONFIG", "")
        .env("value", "")
        .output()?;
    ensure!(
        clean_tool.status.success(),
        "clean profile was shadowed: {clean_tool:?}"
    );
    ensure!(
        String::from_utf8_lossy(&clean_tool.stdout) == "declared-profile|||",
        "wrong clean tool executed"
    );
    fs::remove_file(tools.join("rustc"))?;
    let incomplete = invoke(activate)
        .env_remove("CODEQL_PATH_BOUNDARY")
        .env("DEVENV_PROFILE", &profile)
        .env("PATH", std::env::join_paths([&tools, &runner])?)
        .env("CODEQL_EXPORT_VARIABLES", "")
        .output()?;
    ensure!(
        !incomplete.status.success(),
        "clean devenv used ambient rustc instead of its incomplete profile"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_calls_reject_unknown_missing_and_mistyped_inputs() {
        let definitions = json!({"inputs":{"languages":{"type":"string","required":true},"upload-sarif":{"type":"boolean"}},"secrets":{"token":{"required":false}}});
        assert!(inputs(&json!({"with":{"languages":"[\"rust\"]","upload-sarif":false},"secrets":{"token":"secret"}}), &definitions).is_ok());
        for call in [
            json!({}),
            json!({"with":{"langauges":"[]"}}),
            json!({"with":{"languages":[]}}),
            json!({"with":{"languages":"[]","upload-sarif":"false"}}),
            json!({"with":{"languages":"[]"},"secrets":{"unexpected":"secret"}}),
        ] {
            assert!(inputs(&call, &definitions).is_err());
        }
    }
}
