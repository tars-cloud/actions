use anyhow::{Result, ensure};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;

pub(super) fn system(root: &Path, scratch: &Path, system: &str) -> Result<()> {
    system_with(root, scratch, system, &mut crate::process::run)
}

fn system_with(
    root: &Path,
    scratch: &Path,
    system: &str,
    execute: &mut super::Execute<'_>,
) -> Result<()> {
    let output = scratch.join("output");
    fs::write(&output, "")?;
    let arch = match std::env::consts::ARCH {
        "x86_64" => "X64",
        "aarch64" => "ARM64",
        other => anyhow::bail!("unsupported architecture: {other}"),
    };
    let direct = scratch.join("direct");
    fs::create_dir(&direct)?;
    for name in ["devenv.yaml", "devenv.lock"] {
        fs::copy(root.join(name), direct.join(name))?;
    }
    fs::write(
        direct.join("devenv.nix"),
        "{ pkgs, ... }: { packages = [ pkgs.bash ]; cachix = { enable = false; }; env = { FIXTURE_SYSTEM = pkgs.stdenv.hostPlatform.system; }; }\n",
    )?;
    let flake = root.join("tests/fixtures/flakes");
    for (kind, directory, selector) in [
        ("devenv", &direct, ".#default"),
        ("flakes", &flake, ".#default"),
        ("flakes", &flake, ".#named"),
    ] {
        for script in [
            "composite/setup-devenv/scripts/devenv.sh",
            "composite/run-devenv/scripts/run.sh",
        ] {
            fs::write(&output, "")?;
            let result = execute(Command::new("bash")
                .arg(root.join(script))
                .env("RUNNER_OS", "Linux")
                .env("RUNNER_ARCH", arch)
                .env("RUNNER_ENVIRONMENT", "self-hosted")
                .env("GITHUB_WORKSPACE", root)
                .env("GITHUB_OUTPUT", &output)
                .env("PROJECT_DIRECTORY", directory)
                .env("ENVIRONMENT_TYPE", kind)
                .env("ENVIRONMENT_SYSTEM", system)
                .env("FLAKE_SHELL", selector)
                .env("EXPECTED_SYSTEM", system)
                .env("DEVENV_RUN", "test \"$FIXTURE_SYSTEM\" = \"$EXPECTED_SYSTEM\"; test \"${MACHTYPE%%-*}\" = \"${EXPECTED_SYSTEM%-linux}\""), scratch, 900)?;
            ensure!(
                result.code == Some(0),
                "{system}/{kind}/{selector}/{script}: {}",
                result.text
            );
            if script.contains("setup-devenv") {
                let values = fs::read_to_string(&output)?;
                ensure!(
                    values
                        .lines()
                        .any(|line| line == format!("system={system}")),
                    "setup system output: {values}"
                );
                ensure!(
                    values
                        .lines()
                        .any(|line| line.starts_with("devenv-version=")
                            && (line == "devenv-version=") == (kind == "flakes")),
                    "setup CLI version output: {values}"
                );
            }
        }
    }
    println!("PASS real {system} execution: direct, default and named flake shells");
    Ok(())
}

pub(super) fn run(root: &Path, scratch: &Path, direct_only: bool) -> Result<()> {
    run_with(root, scratch, direct_only, &mut crate::process::run)
}

fn run_with(
    root: &Path,
    scratch: &Path,
    direct_only: bool,
    execute: &mut super::Execute<'_>,
) -> Result<()> {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "X64",
        "aarch64" => "ARM64",
        other => anyhow::bail!("unsupported architecture: {other}"),
    };
    let output = scratch.join("output");
    fs::write(&output, "")?;
    let mut invoke = |script: &str, directory: &Path, kind: &str, selector: &str, path: &str| {
        execute(
            Command::new("bash")
                .arg(root.join(script))
                .env("RUNNER_OS", "Linux")
                .env("RUNNER_ARCH", arch)
                .env("RUNNER_ENVIRONMENT", "self-hosted")
                .env("GITHUB_WORKSPACE", root)
                .env("GITHUB_OUTPUT", &output)
                .env("PROJECT_DIRECTORY", directory)
                .env("ENVIRONMENT_TYPE", kind)
                .env("FLAKE_SHELL", selector)
                .env("PATH", path),
            scratch,
            900,
        )
    };
    let path = std::env::var("PATH")?;
    let real = invoke(
        "composite/setup-trivy/scripts/trivy.sh",
        root,
        "devenv",
        ".#default",
        &path,
    )?;
    ensure!(real.code == Some(0), "declared Trivy: {}", real.text);
    let ambient = scratch.join("ambient");
    fs::create_dir(&ambient)?;
    // A real executable trap is enough; no generated shell test script is needed.
    symlink(crate::runner::executable("false")?, ambient.join("trivy"))?;
    let path = format!("{}:{path}", ambient.display());
    let missing = scratch.join("direct");
    fs::create_dir(&missing)?;
    for name in ["devenv.yaml", "devenv.lock"] {
        fs::copy(root.join(name), missing.join(name))?;
    }
    fs::write(
        missing.join("devenv.nix"),
        "{ pkgs, ... }: { packages = [ pkgs.bash ]; cachix = { enable = false; }; }\n",
    )?;
    let rejected = invoke(
        "composite/setup-trivy/scripts/trivy.sh",
        &missing,
        "devenv",
        ".#default",
        &path,
    )?;
    ensure!(
        rejected.code != Some(0) && rejected.text.contains("Add pkgs.trivy"),
        "missing direct Trivy: {}",
        rejected.text
    );
    println!("PASS real direct environment: declared Trivy accepted, ambient Trivy rejected");
    if direct_only {
        return Ok(());
    }
    let fixture = root.join("tests/fixtures/flakes");
    for name in ["default", "named"] {
        let selector = format!("path:{}#{name}", fixture.display());
        for script in [
            "composite/setup-devenv/scripts/devenv.sh",
            "composite/setup-trivy/scripts/trivy.sh",
        ] {
            let result = invoke(script, &fixture, "flakes", &selector, &path)?;
            ensure!(
                result.code == Some(0),
                "flake {name}/{script}: {}",
                result.text
            );
        }
    }
    let rejected = invoke(
        "composite/setup-trivy/scripts/trivy.sh",
        &fixture,
        "flakes",
        &format!("path:{}#missing-trivy", fixture.display()),
        &path,
    )?;
    ensure!(
        rejected.code != Some(0) && rejected.text.contains("Add pkgs.trivy"),
        "missing flake Trivy: {}",
        rejected.text
    );
    println!("PASS real flake environments: default, named and missing Trivy");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integration::command_env;

    #[test]
    fn checks_selected_system_in_direct_default_and_named_shells() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let scratch = root.join(".tars/scratch/system-orchestration");
        fs::create_dir_all(&scratch)?;
        let fixture = tempfile::tempdir_in(scratch)?;
        let mut observed = Vec::new();
        system_with(
            &root,
            fixture.path(),
            "aarch64-linux",
            &mut |command, _, _| {
                let mode = command_env(command, "ENVIRONMENT_TYPE");
                assert_eq!(command_env(command, "ENVIRONMENT_SYSTEM"), "aarch64-linux");
                assert_eq!(command_env(command, "EXPECTED_SYSTEM"), "aarch64-linux");
                observed.push((mode.clone(), command_env(command, "FLAKE_SHELL")));
                let version = if mode == "flakes" { "" } else { "2.0.0" };
                fs::write(
                    command_env(command, "GITHUB_OUTPUT"),
                    format!("system=aarch64-linux\ndevenv-version={version}\n"),
                )?;
                Ok(crate::process::ResultOutput {
                    code: Some(0),
                    text: String::new(),
                })
            },
        )?;
        assert_eq!(observed.len(), 6);
        assert_eq!(observed[0], ("devenv".into(), ".#default".into()));
        assert_eq!(observed[4], ("flakes".into(), ".#named".into()));
        let fixture = tempfile::tempdir_in(fixture.path())?;
        let error = system_with(&root, fixture.path(), "x86_64-linux", &mut |_, _, _| {
            Ok(crate::process::ResultOutput {
                code: Some(7),
                text: "wrong system".into(),
            })
        })
        .unwrap_err();
        assert!(error.to_string().contains("wrong system"));
        Ok(())
    }

    #[test]
    fn requires_declared_trivy_and_checks_default_named_and_missing_flakes() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let scratch = root.join(".tars/scratch/environment-orchestration");
        fs::create_dir_all(&scratch)?;
        for direct_only in [true, false] {
            let fixture = tempfile::tempdir_in(&scratch)?;
            let mut observed = Vec::new();
            run_with(&root, fixture.path(), direct_only, &mut |command, _, _| {
                let directory = command_env(command, "PROJECT_DIRECTORY");
                let selector = command_env(command, "FLAKE_SHELL");
                let missing = directory == fixture.path().join("direct").to_string_lossy()
                    || selector.ends_with("#missing-trivy");
                observed.push((command_env(command, "ENVIRONMENT_TYPE"), selector));
                Ok(crate::process::ResultOutput {
                    code: Some(if missing { 1 } else { 0 }),
                    text: if missing {
                        "Add pkgs.trivy".into()
                    } else {
                        String::new()
                    },
                })
            })?;
            assert_eq!(observed.len(), if direct_only { 2 } else { 7 });
            assert_eq!(observed[0].0, "devenv");
            if !direct_only {
                assert!(observed.last().unwrap().1.ends_with("#missing-trivy"));
            }
        }
        Ok(())
    }
}
