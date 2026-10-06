use anyhow::{Context, Result, ensure};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

pub(super) fn run(root: &Path) -> Result<()> {
    for (host, consumer) in [
        ("x86_64", "aarch64"),
        ("x86_64", "x86_64"),
        ("aarch64", "aarch64"),
    ] {
        platform_case(root, host, consumer)?;
    }
    println!("PASS CodeQL platform: native launcher utilities and selected consumer compilers");
    Ok(())
}

fn platform_case(root: &Path, host: &str, consumer: &str) -> Result<()> {
    let workflow = super::load(root, ".github/workflows/consumer-codeql.yaml")?;
    let activate = super::step(&workflow, "analyze", "environment")?["with"]["run"]
        .as_str()
        .context("CodeQL environment script")?;
    let scratch = root.join(".tars/scratch/codeql");
    fs::create_dir_all(&scratch)?;
    let fixture = tempfile::tempdir_in(scratch)?;
    let runner = fixture.path().join("runner tools");
    let tools = fixture.path().join("consumer tools");
    let boundary = fixture.path().join("boundary");
    let env_file = fixture.path().join("env");
    let path_file = fixture.path().join("path");
    let bash = crate::runner::executable("bash")?;
    for directory in [&runner, &tools] {
        fs::create_dir_all(directory)?;
    }
    write_tool(&runner, "uname", host, &bash)?;
    write_tool(&tools, "uname", consumer, &bash)?;
    for tool in ["cargo", "rustc"] {
        write_tool(&tools, tool, consumer, &bash)?;
    }
    capture_host_tools(&workflow, fixture.path(), &runner, &bash)?;
    let consumer_path = std::env::join_paths([&tools, &boundary, &runner])?;
    let invoke = |script: &str| {
        let mut command = Command::new(&bash);
        command
            .args(["--noprofile", "--norc", "-euo", "pipefail", "-c", script])
            .env_clear()
            .env("PATH", &consumer_path)
            .env("CODEQL_PATH_BOUNDARY", &boundary)
            .env("CODEQL_LANGUAGE", "rust")
            .env("CODEQL_EXPORT_VARIABLES", "")
            .env("RUNNER_TEMP", fixture.path())
            .env("GITHUB_ENV", &env_file)
            .env("GITHUB_PATH", &path_file);
        command
    };
    let missing = invoke(activate)
        .env("RUNNER_TEMP", fixture.path().join("absent"))
        .output()?;
    ensure!(!missing.status.success(), "missing native tools accepted");
    let activated = invoke(activate).output()?;
    ensure!(activated.status.success(), "activation: {activated:?}");
    let paths = fs::read_to_string(&path_file)?;
    let action_path = std::env::join_paths(paths.lines().rev())?;
    let result = invoke("uname -m; cargo --version; rustc --version")
        .env("PATH", &action_path)
        .output()?;
    ensure!(result.status.success(), "CodeQL tools: {result:?}");
    ensure!(
        String::from_utf8(result.stdout)? == format!("{host}\n{consumer}\n{consumer}\n"),
        "CodeQL launcher must detect {host} while retaining {consumer} consumer compilers"
    );
    let manual = invoke("uname -m").output()?;
    ensure!(
        String::from_utf8(manual.stdout)? == format!("{consumer}\n"),
        "manual consumer shell lost its selected platform"
    );
    Ok(())
}

fn capture_host_tools(
    workflow: &serde_json::Value,
    root: &Path,
    runner: &Path,
    bash: &Path,
) -> Result<()> {
    let step = super::step(workflow, "analyze", "capture_host_tools")?;
    ensure!(
        step["if"] == "(matrix.type || inputs.type) != 'runner'",
        "runner compatibility mode must skip the Nix platform bridge"
    );
    for tool in ["mkdir", "ln"] {
        std::os::unix::fs::symlink(crate::runner::executable(tool)?, runner.join(tool))?;
    }
    let captured = Command::new(bash)
        .args([
            "-euo",
            "pipefail",
            "-c",
            step["run"].as_str().context("capture script")?,
        ])
        .env_clear()
        .env("PATH", runner)
        .env("RUNNER_TEMP", root)
        .output()?;
    ensure!(captured.status.success(), "capture: {captured:?}");
    ensure!(
        fs::read_link(root.join("tars-codeql-host-tools/uname"))? == runner.join("uname"),
        "native uname was not captured"
    );
    Ok(())
}

fn write_tool(directory: &Path, name: &str, value: &str, bash: &Path) -> Result<()> {
    let path = directory.join(name);
    fs::write(
        &path,
        format!("#!{}\nprintf '%s\\n' '{value}'\n", bash.display()),
    )?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_codeql_platform_preserves_consumer_compilers() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        super::run(&root).unwrap();
    }
}
