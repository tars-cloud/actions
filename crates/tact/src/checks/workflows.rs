use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

fn load(root: &Path, name: &str) -> Result<Value> {
    Ok(serde_norway::from_str(&fs::read_to_string(
        root.join(name),
    )?)?)
}

fn step<'a>(workflow: &'a Value, job: &str, id: &str) -> Result<&'a Value> {
    workflow["jobs"][job]["steps"]
        .as_array()
        .context("workflow steps")?
        .iter()
        .find(|step| step["id"] == id)
        .with_context(|| format!("missing workflow step {job}/{id}"))
}

fn inputs(call: &Value, definitions: &Value) -> Result<()> {
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
    for name in ["trivy", "codeql"] {
        let workflow = load(root, &format!(".github/workflows/{name}.yml"))?;
        ensure!(
            workflow["on"]["workflow_call"].is_object(),
            "missing workflow_call"
        );
        for job in workflow["jobs"]
            .as_object()
            .context("workflow jobs")?
            .values()
        {
            for step in job["steps"].as_array().context("steps")? {
                ensure!(
                    step["id"].is_string() && step["name"].is_string(),
                    "step identity"
                );
                if let Some(reference) = step["uses"].as_str() {
                    if let Some(path) = reference.strip_prefix("$/") {
                        let metadata = load(root, &format!("{path}/action.yml"))?;
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
            let reference = call["uses"]
                .as_str()
                .context("example workflow reference")?;
            let (path, revision) = reference
                .strip_prefix("tars-cloud/actions/")
                .context("example owner")?
                .split_once('@')
                .context("example revision")?;
            ensure!(
                revision == "v2"
                    || (revision.len() == 40 && revision.chars().all(|c| c.is_ascii_hexdigit())),
                "example needs the next release alias or full SHA"
            );
            let target = load(root, path)?;
            inputs(call, &target["on"]["workflow_call"])?;
            found |= path == format!(".github/workflows/{name}.yml");
        }
        ensure!(found, "example must exercise its reusable workflow");
    }
    let trivy = load(root, ".github/workflows/trivy.yml")?;
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
            for input in ["type", "working-directory", "flake-shell", "system"] {
                ensure!(
                    s["with"][input] == format!("${{{{ inputs.{input} }}}}"),
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
                .is_some_and(|v| v.contains("steps.configuration.outputs.trusted == 'true'")),
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
    let codeql = load(root, ".github/workflows/codeql.yml")?;
    ensure!(
        !codeql.to_string().contains("setup-devenv") && !codeql.to_string().contains("run-devenv"),
        "CodeQL is independent of devenv"
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
    let ci = load(root, ".github/workflows/security-tests.yml")?;
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

fn execute(root: &Path, workflow: &Value, job: &str, id: &str, mut case: Value) -> Result<()> {
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
    contracts(root)?;
    let codeql = load(root, ".github/workflows/codeql.yml")?;
    let base = json!({
        "env":{"RUNNER_OS":"Linux","LANGUAGE":"actions","BUILD_MODE":"","BUILD_COMMAND":"","CONFIG_FILE":"auto"},
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
    ] {
        let mut case = base.clone();
        case["env"][key] = json!(value);
        case["expect"] = json!({"exit":1,"calls":[],"files":{"injected":null}});
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
    for id in ["setup", "build"] {
        let key = if id == "setup" {
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
        json!({"expect":{"exit":1,"calls":[]}}),
    )?;
    execute(
        root,
        &codeql,
        "analyze",
        "toolchain",
        json!({"commands":{"cargo":[],"rustup":[]},"expect":{"exit":0,"calls":[]}}),
    )?;

    let trivy = load(root, ".github/workflows/trivy.yml")?;
    let base = json!({
        "tools":["realpath","mktemp"],
        "env":{"PROJECT_DIRECTORY":"project","CONFIG_FILE":"auto","SCAN_PATH":".","REPOSITORY":"example/consumer","HEAD_REPOSITORY":"","ACTOR":"developer","PR_AUTHOR":"","RUNNER_TEMP":"${state}/tmp"},
        "files":{"project/input":"fixture"},
        "expect":{"exit":0,"calls":[]}
    });
    // A deterministic mock temp path makes the exact exported paths and trust decision observable.
    for (actor, author, head, trusted) in [
        ("developer", "", "", "true"),
        ("developer", "developer", "EXAMPLE/Consumer", "true"),
        ("dependabot[bot]", "", "", "false"),
        ("developer", "dependabot[bot]", "example/consumer", "false"),
        ("developer", "contributor", "fork/consumer", "false"),
    ] {
        let mut case = base.clone();
        case["tools"] = json!(["realpath"]);
        case["commands"] = json!({"mktemp":[{"args":["-d","${state}/tmp/trivy-XXXXXXXX"],"stdout":"${state}/tmp/report\n","exit":0}]});
        case["env"]["ACTOR"] = json!(actor);
        case["env"]["PR_AUTHOR"] = json!(author);
        case["env"]["HEAD_REPOSITORY"] = json!(head);
        case["files"]["project/trivy.yaml"] = json!("scan: {scanners: [secret]}");
        case["expect"] = json!({"exit":0,"calls":[{"command":"mktemp","args":["-d","${state}/tmp/trivy-XXXXXXXX"]}],"github-output":{"config":"${workspace}/project/trivy.yaml","target":"${workspace}/project","sarif":"${state}/tmp/report/results.sarif","trusted":trusted}});
        execute(root, &trivy, "scan", "configuration", case)?;
    }
    for (key, value) in [
        ("CONFIG_FILE", "missing.yaml"),
        ("CONFIG_FILE", "../outside"),
        ("SCAN_PATH", ".."),
        ("PROJECT_DIRECTORY", ".."),
        ("SCAN_PATH", "name\ninjected=true"),
    ] {
        let mut case = base.clone();
        case["env"][key] = json!(value);
        case["files"]["outside"] = json!("fixture");
        case["expect"]["exit"] = json!(1);
        execute(root, &trivy, "scan", "configuration", case)?;
    }
    for (config, gate, exit) in [
        ("", "false", 0),
        ("config with spaces.yaml", "true", 0),
        ("literal $(touch injected).yaml", "true", 1),
        ("", "false", 2),
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
            if gate == "true" { "1" } else { "0" },
            "--",
            "target with spaces"
        ]);
        execute(
            root,
            &trivy,
            "scan",
            "scan",
            json!({
                "env":{"SCAN_CONFIG":config,"FAIL_ON_FINDINGS":gate,"TRIVY_CACHE_DIR":"cache with spaces","SARIF_FILE":"report.sarif","SCAN_TARGET":"target with spaces"},
                "commands":{"trivy":[{"args":args,"exit":exit}]},
                "expect":{"exit":exit,"calls":[{"command":"trivy","args":args}],"files":{"injected":null}}
            }),
        )?;
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
    println!(
        "PASS reusable workflow scripts: language modes, config discovery, trust, scan arguments, failures and cleanup"
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
