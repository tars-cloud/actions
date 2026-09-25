use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::ResultScenario;

struct Fixture {
    directory: tempfile::TempDir,
    script: PathBuf,
}

impl Fixture {
    fn new(root: &Path) -> Result<Self> {
        let scratch = root.join(".tars/scratch/results");
        fs::create_dir_all(&scratch)?;
        Ok(Self {
            directory: tempfile::Builder::new()
                .prefix("result fixture ")
                .tempdir_in(scratch)?,
            script: root.join("composite/run-devenv/scripts/result/main.cjs"),
        })
    }

    fn invoke(&self, phase: &str, file: &Path, outcome: &str) -> Result<Output> {
        let output = self.directory.path().join("output");
        fs::write(&output, "")?;
        Ok(Command::new(crate::runner::executable("node")?)
            .arg(&self.script)
            .env_clear()
            .env("RUNNER_TEMP", self.directory.path())
            .env("GITHUB_OUTPUT", output)
            .env("INPUT_PHASE", phase)
            .env("INPUT_PATH", file)
            .env("INPUT_OUTCOME", outcome)
            .output()?)
    }

    fn outputs(&self) -> Result<std::collections::BTreeMap<String, String>> {
        crate::output::values(&fs::read_to_string(self.directory.path().join("output"))?)
    }

    fn prepare(&self) -> Result<PathBuf> {
        let result = self.invoke("prepare", Path::new(""), "")?;
        ensure!(result.status.success(), "prepare: {result:?}");
        ensure!(
            result.stdout.is_empty() && result.stderr.is_empty(),
            "unexpected prepare logs"
        );
        let file = PathBuf::from(self.outputs()?.get("path").context("result path")?);
        ensure!(
            !file.exists(),
            "prepare must leave the optional file absent"
        );
        ensure!(file.is_absolute(), "relative result path");
        Ok(file)
    }

    fn collect(&self, file: &Path) -> Result<Value> {
        let result = self.invoke("collect", file, "success")?;
        ensure!(result.status.success(), "collect: {result:?}");
        ensure!(
            result.stdout.is_empty() && result.stderr.is_empty(),
            "result contents leaked to logs"
        );
        ensure!(
            !file.parent().unwrap().exists(),
            "temporary result directory retained"
        );
        let outputs = self.outputs()?;
        ensure!(outputs.len() == 1, "unexpected output injection");
        Ok(serde_json::from_str(
            outputs.get("result").context("JSON result")?,
        )?)
    }
}

pub(super) fn run(root: &Path, scenario: &ResultScenario) -> Result<()> {
    let f = Fixture::new(root)?;
    match scenario {
        ResultScenario::Success => {
            ensure!(
                f.collect(&f.prepare()?)? == json!({}),
                "missing file default"
            );
            let file = f.prepare()?;
            let value = json!({"binary_name":"$(touch injected)\n::error::not-a-command", "nested":{"count":3,"ready":true,"none":null}, "items":["α", "🙂", "quotes\"\\"]});
            fs::write(&file, serde_json::to_string_pretty(&value)?)?;
            ensure!(f.collect(&file)? == value, "structured result changed");
            let file = f.prepare()?;
            fs::write(&file, "{\"integer\":9007199254740993}")?;
            ensure!(
                f.collect(&file)? == json!({"integer":9007199254740993_u64}),
                "integer precision changed"
            );
            let file = f.prepare()?;
            let value = json!({"x":"a".repeat(32760)});
            fs::write(&file, value.to_string())?;
            ensure!(
                f.collect(&file)? == value,
                "exact output size limit rejected"
            );
        }
        ResultScenario::Invalid => {
            for (bytes, diagnostic) in [
                (Vec::new(), "valid UTF-8 JSON"),
                (b"   \n".to_vec(), "valid UTF-8 JSON"),
                (b"{secret-payload".to_vec(), "valid UTF-8 JSON"),
                (b"{} {}".to_vec(), "valid UTF-8 JSON"),
                (vec![0xff], "valid UTF-8 JSON"),
                (b"null".to_vec(), "one JSON object"),
                (b"[]".to_vec(), "one JSON object"),
                (b"42".to_vec(), "one JSON object"),
                (b"\"text\"".to_vec(), "one JSON object"),
                (
                    format!("{{\"x\":\"{}\"}}", "a".repeat(65536)).into_bytes(),
                    "64 KiB file limit",
                ),
                (
                    format!("{{\"x\":\"{}\"}}", "a".repeat(32768)).into_bytes(),
                    "64 KiB UTF-16 output limit",
                ),
            ] {
                let file = f.prepare()?;
                fs::write(&file, bytes)?;
                let result = f.invoke("collect", &file, "success")?;
                ensure!(!result.status.success(), "invalid result accepted");
                let text = String::from_utf8_lossy(&result.stdout);
                ensure!(text.contains(diagnostic), "missing diagnostic: {text}");
                ensure!(!text.contains("secret-payload"), "invalid JSON leaked");
                ensure!(f.outputs()?.is_empty(), "invalid result was published");
                ensure!(!file.parent().unwrap().exists(), "invalid result retained");
            }
            let file = f.prepare()?;
            let outside = f.directory.path().join("unrelated");
            fs::write(&outside, "{\"keep\":true}")?;
            symlink(&outside, &file)?;
            ensure!(
                !f.invoke("collect", &file, "success")?.status.success(),
                "symlink accepted"
            );
            ensure!(
                fs::read_to_string(&outside)? == "{\"keep\":true}",
                "cleanup changed unrelated file"
            );
            ensure!(
                !f.invoke("collect", &outside, "success")?.status.success(),
                "arbitrary cleanup path accepted"
            );
            ensure!(outside.exists(), "unrelated file removed");
        }
        ResultScenario::Isolation => {
            let first = f.prepare()?;
            let second = f.prepare()?;
            ensure!(first != second, "reused result path");
            fs::write(&first, "{\"invocation\":1}")?;
            fs::write(&second, "{\"invocation\":2}")?;
            ensure!(
                f.collect(&first)? == json!({"invocation":1}),
                "first result changed"
            );
            ensure!(second.exists(), "cleanup crossed invocation boundaries");
            ensure!(
                f.collect(&second)? == json!({"invocation":2}),
                "second result changed"
            );
            ensure!(
                f.collect(&f.prepare()?)? == json!({}),
                "stale result restored"
            );
        }
        ResultScenario::Failure => {
            for outcome in ["failure", "cancelled", "skipped"] {
                let file = f.prepare()?;
                fs::write(&file, "partial-secret-json")?;
                let result = f.invoke("collect", &file, outcome)?;
                ensure!(
                    result.status.success(),
                    "collector obscured command failure"
                );
                ensure!(
                    result.stdout.is_empty() && result.stderr.is_empty(),
                    "failed result leaked"
                );
                ensure!(f.outputs()?.is_empty(), "failed command published a result");
                ensure!(!file.parent().unwrap().exists(), "failed result retained");
            }
        }
    }
    Ok(())
}
