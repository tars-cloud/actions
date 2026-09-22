use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::manifest::{Call, Case};

pub(crate) fn dispatch() -> Option<Result<u8>> {
    let executable = std::env::args_os().next()?;
    let name = Path::new(&executable).file_name()?;
    if name == "tact" {
        return None;
    }
    let state = std::env::var_os("TACT_STATE")?;
    // Bash may pass only the command name as argv[0] after a PATH lookup.
    let path = Path::new(&state).join("mock-bin").join(name);
    Some(respond(&path))
}

fn respond(path: &Path) -> Result<u8> {
    let command = path
        .file_name()
        .context("mock command name")?
        .to_string_lossy();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let state = std::env::var("TACT_STATE").context("mock state directory")?;
    let state = Path::new(&state);
    let case: Case = serde_json::from_slice(&fs::read(state.join("case.json"))?)?;
    let expected = case
        .expect
        .calls
        .iter()
        .find(|call| call.command == command && call.args == args);
    let call = Call {
        command: command.to_string(),
        args,
        cwd: expected
            .and_then(|call| call.cwd.as_ref())
            .map(|_| std::env::current_dir().map(|p| p.display().to_string()))
            .transpose()?,
        env: expected
            .map(|call| {
                call.env
                    .keys()
                    .map(|name| (name.clone(), std::env::var(name).unwrap_or_default()))
                    .collect()
            })
            .unwrap_or_default(),
    };
    let mut trace = OpenOptions::new()
        .append(true)
        .open(state.join("calls.jsonl"))?;
    writeln!(trace, "{}", serde_json::to_string(&call)?)?;
    let response = case
        .commands
        .get(command.as_ref())
        .and_then(|responses| responses.iter().find(|response| response.args == call.args));
    let Some(response) = response else {
        let message = format!("unexpected mock call: {call:?}");
        writeln!(
            OpenOptions::new().append(true).open(state.join("errors"))?,
            "{message}"
        )?;
        bail!("{message}");
    };
    print!("{}", response.stdout);
    eprint!("{}", response.stderr);
    if let Some(index) = response.forward {
        let args = call
            .args
            .get(index..)
            .filter(|args| !args.is_empty())
            .context("forward requires a command argument")?;
        let mut child = Command::new(&args[0]);
        child.args(&args[1..]);
        if !response.prepend_path.is_empty() {
            let mut paths = response
                .prepend_path
                .iter()
                .map(std::path::PathBuf::from)
                .collect::<Vec<_>>();
            paths.extend(std::env::split_paths(
                &std::env::var_os("PATH").unwrap_or_default(),
            ));
            child.env("PATH", std::env::join_paths(paths)?);
        }
        return Err(child.exec().into());
    }
    Ok(response.exit)
}
