use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::process::Command;

pub(super) fn contracts(root: &Path) -> Result<()> {
    for entry in fs::read_dir(root.join("composite"))? {
        let directory = entry?.path();
        if !directory.join("action.yaml").is_file() {
            continue;
        }
        let action = load(&directory.join("action.yaml"))?;
        ensure!(
            action["inputs"]["debug"]["default"] == "false",
            "debug default: {directory:?}"
        );
        check_steps(root, &action["runs"]["steps"], true)?;
    }
    for entry in fs::read_dir(root.join(".github/workflows"))? {
        let path = entry?.path();
        let consumer = path
            .file_name()
            .context("workflow filename")?
            .to_string_lossy()
            .starts_with("consumer-");
        let workflow = load(&path)?;
        if !workflow["on"]["workflow_call"].is_object() {
            continue;
        }
        let input = &workflow["on"]["workflow_call"]["inputs"]["debug"];
        ensure!(
            input["type"] == "boolean" && input["default"] == false,
            "workflow debug input: {path:?}"
        );
        for job in workflow["jobs"].as_object().context("jobs")?.values() {
            if consumer && !job["steps"].is_null() {
                check_steps(root, &job["steps"], false)?;
            }
        }
    }
    Ok(())
}

fn load(path: &Path) -> Result<Value> {
    Ok(serde_norway::from_str(&fs::read_to_string(path)?)?)
}

fn check_steps(root: &Path, steps: &Value, composite: bool) -> Result<()> {
    for step in steps.as_array().context("steps")? {
        if let Some(target) = step["uses"]
            .as_str()
            .and_then(|s| s.strip_prefix("$/composite/"))
            && !target.contains('/')
            && root
                .join("composite")
                .join(target)
                .join("action.yaml")
                .is_file()
        {
            ensure!(
                step["with"]["debug"] == "${{ inputs.debug }}",
                "debug forwarding: {target}/{}",
                step["id"]
            );
        }
        if composite && step["shell"] == "bash" {
            ensure!(
                step["env"]["TARS_DEBUG_INPUT"] == "${{ inputs.debug }}",
                "missing raw debug input: {}",
                step["id"]
            );
            ensure!(
                step["env"]["TARS_DEBUG"] == "${{ inputs.debug == 'true' || runner.debug == '1' }}",
                "missing GitHub debug fallback: {}",
                step["id"]
            );
        }
    }
    Ok(())
}

pub(super) fn run(root: &Path) -> Result<()> {
    contracts(root)?;
    let scratch = root.join(".tars/scratch/debug");
    fs::create_dir_all(&scratch)?;
    let bash = crate::runner::executable("bash")?;
    native_shells(root, &scratch, &bash)?;
    let (system, arch) = match std::env::consts::ARCH {
        "x86_64" => ("x86_64-linux", "X64"),
        "aarch64" => ("aarch64-linux", "ARM64"),
        _ => anyhow::bail!("unsupported debug test architecture"),
    };
    for owner in ["setup-devenv", "run-devenv", "setup-trivy", "cargo-crap"] {
        for mode in ["devenv", "flakes", "test"] {
            if mode == "test" && owner != "run-devenv" {
                continue;
            }
            for (input, runner_debug, resolved, enabled) in [
                ("", "", "false", false),
                ("false", "", "false", false),
                ("true", "", "true", true),
                ("", "1", "false", true),
                ("false", "1", "false", true),
                ("false", "", "true", true),
            ] {
                for scenario in ["success", "failure", "pipeline", "startup"] {
                    let fixture = tempfile::tempdir_in(&scratch)?;
                    let directory = fixture.path();
                    let bin = directory.join("bin");
                    fs::create_dir(&bin)?;
                    for tool in ["bash", "dirname"] {
                        symlink(crate::runner::executable(tool)?, bin.join(tool))?;
                    }
                    for file in [
                        "devenv.nix",
                        "devenv.yaml",
                        "devenv.lock",
                        "flake.nix",
                        "flake.lock",
                    ] {
                        fs::write(directory.join(file), "{}")?;
                    }
                    for tool in ["nix", "devenv"] {
                        let file = bin.join(tool);
                        fs::write(&file, format!("#!{}\n{}", bash.display(), MOCK))?;
                        fs::set_permissions(file, fs::Permissions::from_mode(0o700))?;
                    }
                    let mut command = Command::new(&bash);
                    if owner == "run-devenv" {
                        command.arg(root.join("composite/run-devenv/scripts/run.sh"));
                    } else {
                        command
                            .args([
                                "-euo",
                                "pipefail",
                                "-c",
                                "source \"$1\"; debug_start dispatch; dispatch -c \"$DEVENV_RUN\"",
                                "debug-test",
                            ])
                            .arg(root.join(format!("composite/{owner}/scripts/environment.sh")));
                    }
                    let source = match scenario {
                        "failure" => "exit 17",
                        "pipeline" => "false | true; printf unexpected",
                        _ => {
                            "printf 'live stdout\\n'; printf 'live stderr\\n' >&2; printf '{\"ok\":true}' > \"$DEVENV_RESULT_FILE\"; : \"$CREDENTIAL\""
                        }
                    };
                    let output = command
                        .current_dir(directory)
                        .env_clear()
                        .env("PATH", &bin)
                        .env("GITHUB_WORKSPACE", directory)
                        .env("RUNNER_OS", "Linux")
                        .env("RUNNER_ARCH", arch)
                        .env("RUNNER_ENVIRONMENT", "self-hosted")
                        .env(
                            "ENVIRONMENT_TYPE",
                            if mode == "test" { "devenv" } else { mode },
                        )
                        .env("ENVIRONMENT_SYSTEM", system)
                        .env("FLAKE_SHELL", ".#named")
                        .env("DEVENV_MODE", if mode == "test" { "test" } else { "shell" })
                        .env("TARS_DEBUG_INPUT", input)
                        .env("RUNNER_DEBUG", runner_debug)
                        .env("TARS_DEBUG", resolved)
                        .env("DEVENV_RUN", source)
                        .env("SCENARIO", scenario)
                        .env("DEVENV_RESULT_FILE", directory.join("result.json"))
                        .env("ARGUMENTS", directory.join("arguments"))
                        .env("CREDENTIAL", "fixture-credential-never-log")
                        .env(
                            "NIX_CONFIG",
                            "extra-access-tokens = github.com=fixture-credential-never-log",
                        )
                        .output()?;
                    let status = match scenario {
                        "failure" => 17,
                        "pipeline" => 1,
                        "startup" => 126,
                        _ => 0,
                    };
                    ensure!(
                        output.status.code() == Some(status),
                        "{owner}/{mode}/{input}/{runner_debug}/{scenario}: {output:?}"
                    );
                    let stderr = String::from_utf8(output.stderr)?;
                    let stdout = String::from_utf8(output.stdout)?;
                    let arguments = fs::read_to_string(directory.join("arguments"))?;
                    ensure!(
                        arguments.contains("--verbose\n") == enabled,
                        "verbosity: {arguments}"
                    );
                    ensure!(
                        arguments.contains("--quiet\n") == (!enabled && mode == "devenv"),
                        "quiet mode: {arguments}"
                    );
                    ensure!(
                        arguments.contains("--print-build-logs\n") == (enabled && mode == "flakes"),
                        "build logs: {arguments}"
                    );
                    if mode != "flakes" {
                        ensure!(arguments.contains("--no-tui\n"), "TUI enabled");
                    }
                    ensure!(
                        stderr.contains("[tars:debug]") == enabled,
                        "diagnostics: {stderr}"
                    );
                    if enabled {
                        ensure!(
                            stderr.contains(&format!("finished: exit={status} elapsed=")),
                            "phase result: {stderr}"
                        );
                        ensure!(
                            stderr.contains("consumer shell entered")
                                == (scenario != "startup" && mode != "test"),
                            "shell marker: {stderr}"
                        );
                        ensure!(
                            stderr.contains(&format!("selected_system={system}")),
                            "system: {stderr}"
                        );
                    }
                    ensure!(
                        !stderr.contains("fixture-credential-never-log")
                            && !stdout.contains("fixture-credential-never-log"),
                        "credential logged"
                    );
                    ensure!(!stderr.contains(source), "consumer source logged");
                    if scenario == "success" && mode != "test" {
                        ensure!(
                            stdout == "live stdout\n"
                                && fs::read_to_string(directory.join("result.json"))?
                                    == "{\"ok\":true}",
                            "consumer output changed"
                        );
                        if !enabled {
                            ensure!(stderr == "live stderr\n", "default logs changed");
                        }
                    }
                }
            }
        }
    }
    for entry in fs::read_dir(root.join("composite"))? {
        let helper = entry?.path().join("scripts/debug.sh");
        if !helper.is_file() {
            continue;
        }
        for runner_debug in ["", "1"] {
            let output = Command::new(&bash)
                .arg(&helper)
                .env_clear()
                .env("TARS_DEBUG_INPUT", "yes")
                .env("RUNNER_DEBUG", runner_debug)
                .output()?;
            ensure!(
                output.status.code() == Some(1)
                    && String::from_utf8(output.stderr)?.contains("debug must be true or false"),
                "invalid debug input accepted: {helper:?}"
            );
        }
    }
    let cache = super::cache::Fixture::new(root)?;
    cache.write(
        "Cargo.toml",
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\n",
    )?;
    for mode in ["devenv", "flakes"] {
        let normal = cache.plan(json!({"type":mode}), json!({}), json!({}))?;
        let verbose = cache.plan(
            json!({"type":mode,"debug":"true"}),
            json!({}),
            json!({"TARS_DEBUG":"true","RUNNER_DEBUG":"1"}),
        )?;
        ensure!(normal == verbose, "debug changed cache planning in {mode}");
    }
    let report = tempfile::tempdir_in(&scratch)?;
    for (input, runner_debug) in [("true", ""), ("false", "1")] {
        let output = Command::new(&bash)
            .arg(root.join("composite/report-status/scripts/report.sh"))
            .env_clear()
            .env("PATH", "/nonexistent")
            .env("TARS_DEBUG_INPUT", input)
            .env("RUNNER_DEBUG", runner_debug)
            .env("REPORT_RESULTS", "build=success")
            .env("GITHUB_OUTPUT", report.path().join("outputs"))
            .env("GITHUB_STEP_SUMMARY", report.path().join("summary"))
            .output()?;
        ensure!(
            output.status.success()
                && String::from_utf8(output.stderr)?.contains("report-status finished: exit=0"),
            "debug reporting requires external tools"
        );
    }
    Ok(())
}

fn native_shells(root: &Path, scratch: &Path, bash: &Path) -> Result<()> {
    let mut shells = std::collections::BTreeSet::new();
    for entry in fs::read_dir(root.join(".github/workflows"))? {
        let workflow = load(&entry?.path())?;
        for job in workflow["jobs"].as_object().context("jobs")?.values() {
            if let Some(steps) = job["steps"].as_array() {
                for step in steps {
                    if let Some(shell) = step["shell"]
                        .as_str()
                        .filter(|s| s.contains("TARS_DEVENV_LOGGING"))
                    {
                        shells.insert(shell.to_owned());
                    }
                }
            }
        }
    }
    ensure!(
        shells.len() == 1,
        "native workflow shell wrappers must use one contract"
    );
    let shell = shells.first().context("native shell")?;
    // GitHub's process launcher groups double quotes; single quotes are literal characters.
    let quoted = shell
        .strip_prefix("bash --noprofile --norc -e -o pipefail -c ")
        .and_then(|s| s.strip_suffix(" tars-devenv {0}"))
        .context("native shell launcher")?;
    let command: String = serde_json::from_str(quoted).context("GitHub shell command quoting")?;
    let fixture = tempfile::tempdir_in(scratch)?;
    let directory = fixture.path();
    let bin = directory.join("bin");
    fs::create_dir(&bin)?;
    symlink(bash, bin.join("bash"))?;
    let mock = bin.join("devenv");
    fs::write(&mock, format!("#!{}\n{}", bash.display(), MOCK))?;
    fs::set_permissions(&mock, fs::Permissions::from_mode(0o700))?;
    for verbosity in ["--quiet", "--verbose"] {
        for (script, status) in [
            ("printf 'result=value\\n' >> \"$GITHUB_OUTPUT\"", 0),
            ("exit 17", 17),
            ("false | true; printf unexpected", 1),
        ] {
            let source = directory.join("step with spaces.sh");
            fs::write(&source, script)?;
            let outputs = directory.join("outputs");
            fs::write(&outputs, "")?;
            let output = Command::new(bash)
                .args([
                    "--noprofile",
                    "--norc",
                    "-e",
                    "-o",
                    "pipefail",
                    "-c",
                    &command,
                    "tars-devenv",
                ])
                .arg(source)
                .env_clear()
                .env("PATH", &bin)
                .env("TARS_DEVENV_LOGGING", verbosity)
                .env("GITHUB_OUTPUT", &outputs)
                .env("ARGUMENTS", directory.join("arguments"))
                .env("SCENARIO", "success")
                .output()?;
            ensure!(
                output.status.code() == Some(status),
                "native shell failure semantics: {output:?}"
            );
            ensure!(
                fs::read_to_string(directory.join("arguments"))?.contains(verbosity),
                "native logging flag missing"
            );
            ensure!(
                fs::read_to_string(&outputs)? == if status == 0 { "result=value\n" } else { "" },
                "native step outputs changed"
            );
        }
    }
    Ok(())
}

const MOCK: &str = r#"
set -euo pipefail
printf '%s\n' "$@" > "$ARGUMENTS"
if [[ $SCENARIO == startup ]]; then exit 126; fi
for argument in "$@"; do
    if [[ $argument == test ]]; then
        case $SCENARIO in
            failure) exit 17 ;;
            pipeline) exit 1 ;;
            *) exit 0 ;;
        esac
    fi
done
while [[ $1 != -- && $1 != --command ]]; do shift; done
shift
exec "$@"
"#;
