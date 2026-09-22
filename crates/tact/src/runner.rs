use std::fs::{self, File};
use std::os::unix::{fs::symlink, process::CommandExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;

use crate::manifest::{Call, Case, Manifest};
use crate::output;

pub(crate) fn executable(name: &str) -> Result<PathBuf> {
    if name == "tact" {
        return Ok(std::env::current_exe()?);
    }
    for dir in std::env::split_paths(&std::env::var_os("PATH").context("PATH is unset")?) {
        let path = dir.join(name);
        if path.is_file() {
            return Ok(path.canonicalize()?);
        }
    }
    bail!("{name} is missing from the devenv shell")
}

fn copy(source: &Path, target: &Path) -> Result<()> {
    let metadata = source.symlink_metadata()?;
    ensure!(
        !metadata.is_symlink(),
        "source symlinks are unsupported: {}",
        source.display()
    );
    if metadata.is_dir() {
        fs::create_dir_all(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy(&entry.path(), &target.join(entry.file_name()))?;
        }
    } else {
        ensure!(
            metadata.is_file(),
            "unsupported source file: {}",
            source.display()
        );
        fs::create_dir_all(target.parent().context("source parent")?)?;
        fs::copy(source, target)?;
    }
    Ok(())
}

fn equal<T: std::fmt::Debug + PartialEq>(label: &str, actual: T, expected: T) -> Result<()> {
    ensure!(
        actual == expected,
        "{label}: expected {expected:?}, actual {actual:?}"
    );
    Ok(())
}

pub(crate) fn run(root: &Path, manifest: &Manifest, case: &Case) -> Result<()> {
    let scratch = root.join(".tars/scratch/tact");
    fs::create_dir_all(&scratch)?;
    let fixture = tempfile::Builder::new()
        .prefix("case-")
        .tempdir_in(scratch)?;
    let state = fixture.path();
    let workspace = state.join("workspace");
    let bin = state.join("mock-bin");
    let mut value = serde_json::to_value(case)?;
    expand(
        &mut value,
        &[
            ("${workspace}", &workspace),
            ("${home}", &state.join("home")),
            ("${state}", state),
            ("${bin}", &bin),
        ],
    );
    let case: Case = serde_json::from_value(value)?;
    for dir in [&workspace, &bin, &state.join("home"), &state.join("tmp")] {
        fs::create_dir_all(dir)?;
    }
    for source in &manifest.sources {
        let source_path = root.join(source);
        ensure!(
            source_path.canonicalize()?.starts_with(root),
            "source escapes repository: {source}"
        );
        copy(&source_path, &workspace.join(source))?;
    }
    for (path, content) in &case.files {
        let path = workspace.join(path);
        fs::create_dir_all(path.parent().context("fixture parent")?)?;
        fs::write(path, content)?;
    }
    for name in ["bash", "dirname"] {
        symlink(executable(name)?, bin.join(name))?;
    }
    for name in &case.tools {
        symlink(executable(name)?, bin.join(name))?;
    }
    for name in case.commands.keys() {
        let paths = case
            .command_paths
            .get(name)
            .cloned()
            .unwrap_or_else(|| vec![bin.join(name).display().to_string()]);
        for path in paths {
            let path = PathBuf::from(path);
            ensure!(
                path.starts_with(state)
                    && !path
                        .components()
                        .any(|c| c == std::path::Component::ParentDir),
                "mock path must remain inside its fixture"
            );
            fs::create_dir_all(path.parent().context("mock parent")?)?;
            symlink(std::env::current_exe()?, path)?;
        }
    }
    fs::write(state.join("case.json"), serde_json::to_vec(&case)?)?;
    for name in ["calls.jsonl", "errors", "output", "env", "paths"] {
        File::create(state.join(name))?;
    }
    ensure!(
        !case.command[0].contains('/'),
        "command executable must be a name from the fixture PATH"
    );
    let command = bin.join(&case.command[0]);
    ensure!(
        command.is_file(),
        "command {} is not declared in the fixture PATH",
        case.command[0]
    );
    let mut child = Command::new(command)
        .args(&case.command[1..])
        .current_dir(&workspace)
        .env_clear()
        .envs(&case.env)
        .env("PATH", &bin)
        .env("HOME", state.join("home"))
        .env("TMPDIR", state.join("tmp"))
        .env("LC_ALL", "C")
        .env("TZ", "UTC")
        .env("GITHUB_WORKSPACE", &workspace)
        .env("GITHUB_OUTPUT", state.join("output"))
        .env("GITHUB_ENV", state.join("env"))
        .env("GITHUB_PATH", state.join("paths"))
        .env("TACT_STATE", state)
        .stdin(Stdio::null())
        .stdout(File::create(state.join("stdout"))?)
        .stderr(File::create(state.join("stderr"))?)
        .process_group(0)
        .spawn()
        .context("start scenario")?;
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() >= Duration::from_secs(case.timeout_seconds) {
            let _ = killpg(Pid::from_raw(child.id() as i32), Signal::SIGKILL);
            child.wait()?;
            bail!("timed out after {} seconds", case.timeout_seconds);
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    // A completed shell must not leave background children writing into the next case.
    let _ = killpg(Pid::from_raw(child.id() as i32), Signal::SIGKILL);
    let stdout = fs::read_to_string(state.join("stdout"))?;
    let stderr = fs::read_to_string(state.join("stderr"))?;
    let check = || -> Result<()> {
        equal("exit", status.code(), Some(case.expect.exit))?;
        if let Some(expected) = &case.expect.stdout {
            equal("stdout", &stdout, expected)?;
        }
        if let Some(expected) = &case.expect.stderr {
            equal("stderr", &stderr, expected)?;
        }
        for (name, expected) in [
            ("output", &case.expect.github_output),
            ("env", &case.expect.github_env),
        ] {
            if let Some(expected) = expected {
                equal(
                    name,
                    &output::values(&fs::read_to_string(state.join(name))?)?,
                    expected,
                )?;
            }
        }
        if let Some(expected) = &case.expect.github_path {
            let text = fs::read_to_string(state.join("paths"))?;
            equal(
                "github-path",
                text.lines().collect::<Vec<_>>(),
                expected.iter().map(String::as_str).collect(),
            )?;
        }
        let calls = fs::read_to_string(state.join("calls.jsonl"))?
            .lines()
            .map(serde_json::from_str)
            .collect::<Result<Vec<Call>, _>>()?;
        equal("calls", &calls, &case.expect.calls)?;
        let errors = fs::read_to_string(state.join("errors"))?;
        ensure!(errors.is_empty(), "{errors}");
        for (path, expected) in &case.expect.files {
            let path = workspace.join(path);
            match expected {
                Some(expected) => equal(
                    &path.display().to_string(),
                    &fs::read_to_string(&path)?,
                    expected,
                )?,
                None => ensure!(
                    !path.try_exists()?,
                    "expected absent file: {}",
                    path.display()
                ),
            }
        }
        Ok(())
    };
    check().with_context(|| format!("stdout: {stdout:?}\nstderr: {stderr:?}"))
}

fn expand(value: &mut serde_json::Value, paths: &[(&str, &Path)]) {
    match value {
        serde_json::Value::String(text) => {
            for (token, path) in paths {
                *text = text.replace(token, &path.to_string_lossy());
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                expand(value, paths);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values_mut() {
                expand(value, paths);
            }
        }
        _ => {}
    }
}
