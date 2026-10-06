use anyhow::{Result, bail};
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use std::fs::{self, File};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
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
    let status = wait(&mut child, seconds)?;
    let text = fs::read_to_string(log.path().join("stdout"))?
        + &fs::read_to_string(log.path().join("stderr"))?;
    Ok(ResultOutput {
        code: status.code(),
        text,
    })
}

pub(crate) fn wait(child: &mut Child, seconds: u64) -> Result<ExitStatus> {
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() >= Duration::from_secs(seconds) {
            let _ = killpg(Pid::from_raw(child.id() as i32), Signal::SIGKILL);
            // The private process group may be unavailable during timeout cleanup.
            let _ = child.kill();
            child.wait()?;
            bail!("command timed out after {seconds} seconds");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let _ = killpg(Pid::from_raw(child.id() as i32), Signal::SIGKILL);
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_both_streams_and_preserves_nonzero_exit() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        let result = run(
            Command::new(crate::runner::executable("bash")?)
                .args(["-c", "printf stdout; printf stderr >&2; exit 17"]),
            scratch.path(),
            10,
        )?;
        assert_eq!(result.code, Some(17));
        assert_eq!(result.text, "stdoutstderr");
        assert_eq!(fs::read_dir(scratch.path())?.count(), 0);
        Ok(())
    }

    #[test]
    fn timeout_kills_and_reaps_the_child_and_cleans_logs() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        let error = run(
            Command::new(crate::runner::executable("bash")?).args(["-c", "while :; do :; done"]),
            scratch.path(),
            0,
        )
        .err()
        .unwrap();
        assert_eq!(error.to_string(), "command timed out after 0 seconds");
        assert_eq!(fs::read_dir(scratch.path())?.count(), 0);
        Ok(())
    }

    #[test]
    fn timeout_reaps_child_without_a_private_process_group() -> Result<()> {
        let mut child = Command::new(crate::runner::executable("bash")?)
            .args(["-c", "while :; do :; done"])
            .spawn()?;
        let pid = Pid::from_raw(child.id() as i32);
        let (finished, receiver) = std::sync::mpsc::channel();
        // Rescue the child if timeout cleanup hangs, so the regression fails without hanging CI.
        let watchdog = std::thread::spawn(move || {
            if receiver.recv_timeout(Duration::from_secs(2)).is_err() {
                let _ = nix::sys::signal::kill(pid, Signal::SIGKILL);
                return true;
            }
            false
        });
        let error = wait(&mut child, 0).unwrap_err();
        let _ = finished.send(());
        assert!(!watchdog.join().unwrap(), "timeout cleanup needed rescue");
        assert_eq!(error.to_string(), "command timed out after 0 seconds");
        assert!(child.try_wait()?.is_some());
        Ok(())
    }

    #[test]
    fn completion_kills_background_descendants() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        let marker = scratch.path().join("descendant");
        run(
            Command::new(crate::runner::executable("bash")?)
                .args([
                    "-c",
                    "(sleep 0.1; printf leaked > \"$1\") & exit 0",
                    "fixture",
                ])
                .arg(&marker),
            scratch.path(),
            10,
        )?;
        std::thread::sleep(Duration::from_millis(200));
        assert!(!marker.exists());
        Ok(())
    }
}
