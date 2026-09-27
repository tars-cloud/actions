use anyhow::Result;
use std::fs::OpenOptions;
use std::io::Write;

pub(crate) fn note(message: &str) -> Result<()> {
    println!("{message}");
    if let Some(path) = std::env::var_os("GITHUB_STEP_SUMMARY") {
        writeln!(
            OpenOptions::new().create(true).append(true).open(path)?,
            "{message}\n"
        )?;
    }
    Ok(())
}
