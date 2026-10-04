use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::process::Command;

pub(super) fn run(root: &Path, scratch: &Path) -> Result<()> {
    run_with(root, scratch, &mut crate::process::run)
}

fn run_with(root: &Path, scratch: &Path, execute: &mut super::Execute<'_>) -> Result<()> {
    let output = scratch.join("output");
    let helper = |phase: &str, file: &str, execute: &mut super::Execute<'_>| -> Result<()> {
        fs::write(&output, "")?;
        let result = execute(
            Command::new("node")
                .arg(root.join("composite/run-devenv/scripts/result/main.cjs"))
                .env("RUNNER_TEMP", scratch)
                .env("GITHUB_OUTPUT", &output)
                .env("INPUT_PHASE", phase)
                .env("INPUT_PATH", file)
                .env("INPUT_OUTCOME", "success"),
            scratch,
            30,
        )?;
        ensure!(result.code == Some(0), "result {phase}: {}", result.text);
        Ok(())
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "X64",
        "aarch64" => "ARM64",
        other => anyhow::bail!("unsupported architecture: {other}"),
    };
    for (kind, directory, selector) in [
        ("devenv", root.to_path_buf(), ".#default"),
        ("flakes", root.join("tests/fixtures/flakes"), ".#default"),
        ("flakes", root.join("tests/fixtures/flakes"), ".#named"),
    ] {
        helper("prepare", "", execute)?;
        let outputs = crate::output::values(&fs::read_to_string(&output)?)?;
        let file = outputs.get("path").context("allocated result path")?;
        let result = execute(
            Command::new("bash")
                .arg(root.join("composite/run-devenv/scripts/run.sh"))
                .env("RUNNER_OS", "Linux")
                .env("RUNNER_ARCH", arch)
                .env("RUNNER_ENVIRONMENT", "self-hosted")
                .env("GITHUB_WORKSPACE", root)
                .env("GITHUB_OUTPUT", &output)
                .env("PROJECT_DIRECTORY", &directory)
                .env("ENVIRONMENT_TYPE", kind)
                .env("FLAKE_SHELL", selector)
                .env("DEVENV_RESULT_FILE", file)
                .env("DEVENV_RUN", "printf 'result fixture stdout\\n'; printf 'result fixture stderr\\n' >&2; printf '{\"mode\":\"%s\",\"ready\":true}' \"$ENVIRONMENT_TYPE\" > \"$DEVENV_RESULT_FILE\""),
            scratch,
            900,
        )?;
        ensure!(result.code == Some(0), "{kind}/{selector}: {}", result.text);
        ensure!(
            result.text.contains("result fixture stdout")
                && result.text.contains("result fixture stderr"),
            "command logs were lost"
        );
        helper("collect", file, execute)?;
        let outputs = crate::output::values(&fs::read_to_string(&output)?)?;
        let value: Value =
            serde_json::from_str(outputs.get("result").context("published result")?)?;
        ensure!(
            value == json!({"mode":kind,"ready":true}),
            "real result changed"
        );
        ensure!(
            !Path::new(file).parent().unwrap().exists(),
            "real result was not cleaned up"
        );
        println!("PASS real {kind}/{selector} structured result and cleanup");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishes_results_for_each_shell_and_preserves_command_failures() -> Result<()> {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let scratch = repository.join(".tars/scratch/result-orchestration");
        fs::create_dir_all(&scratch)?;
        let fixture = tempfile::tempdir_in(scratch)?;
        let mut shells = Vec::new();
        run_with(
            &repository,
            fixture.path(),
            &mut |command, scratch, seconds| {
                if command.get_program() == "node" {
                    return crate::process::run(command, scratch, seconds);
                }
                let mode = super::super::command_env(command, "ENVIRONMENT_TYPE");
                let selector = super::super::command_env(command, "FLAKE_SHELL");
                let file = super::super::command_env(command, "DEVENV_RESULT_FILE");
                shells.push((mode.clone(), selector));
                fs::write(file, json!({"mode":mode,"ready":true}).to_string())?;
                Ok(crate::process::ResultOutput {
                    code: Some(0),
                    text: "result fixture stdout\nresult fixture stderr\n".into(),
                })
            },
        )?;
        assert_eq!(
            shells,
            [
                ("devenv".into(), ".#default".into()),
                ("flakes".into(), ".#default".into()),
                ("flakes".into(), ".#named".into())
            ]
        );
        let error = run_with(
            &repository,
            fixture.path(),
            &mut |command, scratch, seconds| {
                if command.get_program() == "node" {
                    return crate::process::run(command, scratch, seconds);
                }
                Ok(crate::process::ResultOutput {
                    code: Some(7),
                    text: "shell startup failed".into(),
                })
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("shell startup failed"));
        Ok(())
    }
}
