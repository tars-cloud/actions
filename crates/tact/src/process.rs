use anyhow::{Result, bail};
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use std::fs::{self, File};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub(crate) struct ResultOutput {
    pub code: Option<i32>,
    pub text: String,
}

pub(crate) fn run(command: &mut Command, scratch: &Path, seconds: u64) -> Result<ResultOutput> {
    let log = tempfile::tempdir_in(scratch)?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(File::create(log.path().join("stdout"))?)
        .stderr(File::create(log.path().join("stderr"))?)
        .process_group(0)
        .spawn()?;
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() >= Duration::from_secs(seconds) {
            let _ = killpg(Pid::from_raw(child.id() as i32), Signal::SIGKILL);
            child.wait()?;
            bail!("command timed out after {seconds}s");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let _ = killpg(Pid::from_raw(child.id() as i32), Signal::SIGKILL);
    let text = fs::read_to_string(log.path().join("stdout"))?
        + &fs::read_to_string(log.path().join("stderr"))?;
    Ok(ResultOutput {
        code: status.code(),
        text,
    })
}
