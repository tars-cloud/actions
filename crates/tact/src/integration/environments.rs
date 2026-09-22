use anyhow::{Result, ensure};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;

pub(super) fn run(root: &Path, scratch: &Path, direct_only: bool) -> Result<()> {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "X64",
        "aarch64" => "ARM64",
        other => anyhow::bail!("unsupported architecture: {other}"),
    };
    let output = scratch.join("output");
    fs::write(&output, "")?;
    let invoke = |script: &str, directory: &Path, kind: &str, selector: &str, path: &str| {
        crate::process::run(
            Command::new("bash")
                .arg(root.join("internal").join(script))
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
    let real = invoke("trivy.sh", root, "devenv", ".#default", &path)?;
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
    let rejected = invoke("trivy.sh", &missing, "devenv", ".#default", &path)?;
    ensure!(
        rejected.code != Some(0) && rejected.text.contains("Add pkgs.trivy"),
        "missing direct Trivy: {}",
        rejected.text
    );
    println!("PASS real direct environment: declared Trivy accepted, ambient Trivy rejected");
    if direct_only {
        return Ok(());
    }
    let fixture = root.join("fixtures/flakes");
    for name in ["default", "named"] {
        let selector = format!("path:{}#{name}", fixture.display());
        for script in ["devenv.sh", "trivy.sh"] {
            let result = invoke(script, &fixture, "flakes", &selector, &path)?;
            ensure!(
                result.code == Some(0),
                "flake {name}/{script}: {}",
                result.text
            );
        }
    }
    let rejected = invoke(
        "trivy.sh",
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
