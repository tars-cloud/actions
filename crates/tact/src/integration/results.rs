use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::process::Command;

pub(super) fn run(root: &Path, scratch: &Path) -> Result<()> {
    let output = scratch.join("output");
    let helper = |phase: &str, file: &str| -> Result<()> {
        fs::write(&output, "")?;
        let result = crate::process::run(
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
        helper("prepare", "")?;
        let outputs = crate::output::values(&fs::read_to_string(&output)?)?;
        let file = outputs.get("path").context("allocated result path")?;
        let result = crate::process::run(
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
        helper("collect", file)?;
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
