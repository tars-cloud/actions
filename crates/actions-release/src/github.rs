use anyhow::{Context, Result, ensure};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};

pub(crate) struct Github {
    pub(crate) repo: String,
}

impl Github {
    pub(crate) fn from_env() -> Result<Self> {
        let repo = std::env::var("GITHUB_REPOSITORY").context("GITHUB_REPOSITORY is required")?;
        ensure!(
            repo.split('/').count() == 2
                && repo.split('/').all(|s| !s.is_empty()
                    && s.bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))),
            "invalid repository"
        );
        Ok(Self { repo })
    }

    pub(crate) fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let output = crate::run(
            self.read_command()
                .args(["api", &format!("repos/{}/{path}", self.repo)]),
        )?;
        serde_json::from_str(&output).context("decode GitHub response")
    }

    pub(crate) fn list<T: DeserializeOwned>(&self, path: &str) -> Result<Vec<T>> {
        let output = crate::run(self.read_command().args([
            "api",
            "--paginate",
            "--slurp",
            &format!("repos/{}/{path}", self.repo),
        ]))?;
        let pages: Vec<Vec<T>> = serde_json::from_str(&output).context("decode GitHub pages")?;
        Ok(pages.into_iter().flatten().collect())
    }

    fn read_command(&self) -> Command {
        let mut command = Command::new("gh");
        // CI reads use the job token; the App token only needs Contents and Pull requests write.
        if let Some(token) = std::env::var_os("GH_READ_TOKEN") {
            command.env("GH_TOKEN", token);
        }
        command
    }

    pub(crate) fn write(&self, method: &str, path: &str, body: Value) -> Result<Value> {
        let mut child = Command::new("gh")
            .args([
                "api",
                "--method",
                method,
                &format!("repos/{}/{path}", self.repo),
                "--input",
                "-",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .context("GitHub stdin")?
            .write_all(&serde_json::to_vec(&body)?)?;
        let output = child.wait_with_output()?;
        ensure!(
            output.status.success(),
            "GitHub {method} {path} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    }

    pub(crate) fn create_ref(&self, tag: &str, sha: &str) -> Result<()> {
        self.write(
            "POST",
            "git/refs",
            json!({"ref": format!("refs/tags/{tag}"), "sha": sha}),
        )?;
        Ok(())
    }
}
