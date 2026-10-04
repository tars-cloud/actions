use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn env(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("missing {name}"))
}

fn api(repository: &str, method: &str, path: &str, body: Option<Value>) -> Result<Value> {
    let mut child = Command::new("gh")
        .args([
            "api",
            "--method",
            method,
            &format!("repos/{repository}/{path}"),
        ])
        .args(if body.is_some() {
            vec!["--input", "-"]
        } else {
            vec![]
        })
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(body) = body {
        child
            .stdin
            .take()
            .context("API stdin")?
            .write_all(&serde_json::to_vec(&body)?)?;
    }
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "GitHub {method} {path}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    if output.stdout.is_empty() {
        return Ok(Value::Null);
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn write_output(key: &str, value: &str) -> Result<()> {
    writeln!(
        OpenOptions::new()
            .append(true)
            .open(env("GITHUB_OUTPUT")?)?,
        "{key}={value}"
    )?;
    Ok(())
}

fn successful_probe(runs: &Value) -> Result<bool> {
    Ok(runs["workflow_runs"]
        .as_array()
        .context("workflow runs")?
        .iter()
        .any(|run| {
            run["path"] == ".github/workflows/test-devenv-update-probe.yaml"
                && run["conclusion"] == "success"
        }))
}

pub(crate) fn run(root: &Path, phase: &str) -> Result<()> {
    let repository = env("GITHUB_REPOSITORY")?;
    let identity = format!("{}-{}", env("GITHUB_RUN_ID")?, env("GITHUB_RUN_ATTEMPT")?);
    ensure!(
        identity.chars().all(|c| c.is_ascii_digit() || c == '-'),
        "invalid run identity"
    );
    let base = format!("tact-update-base-{identity}");
    let branch = format!("tact-update-pr-{identity}");
    let lifecycle = Lifecycle {
        root,
        repository: &repository,
        base,
        branch,
    };
    match phase {
        "prepare" => lifecycle.prepare(&env("GITHUB_SHA")?)?,
        "advance" => lifecycle.advance()?,
        "verify" => lifecycle.verify()?,
        "reconcile" => lifecycle.reconcile()?,
        "closed" => lifecycle.closed()?,
        "cleanup" => lifecycle.cleanup()?,
        _ => anyhow::bail!("unknown update lifecycle phase"),
    }
    println!("PASS update lifecycle: {phase}");
    Ok(())
}

struct Lifecycle<'a> {
    root: &'a Path,
    repository: &'a str,
    base: String,
    branch: String,
}

impl Lifecycle<'_> {
    fn prs(&self) -> Result<Value> {
        let base = &self.base;
        api(
            self.repository,
            "GET",
            &format!(
                "pulls?state=all&base={base}&head={}:{}",
                self.repository.split('/').next().context("owner")?,
                self.branch
            ),
            None,
        )
    }

    fn prepare(&self, source: &str) -> Result<()> {
        let base = &self.base;
        let fixture = self.root.join("tests/fixtures/devenv-update");
        let mut lock: Value =
            serde_json::from_str(&fs::read_to_string(fixture.join("devenv.lock"))?)?;
        lock["nodes"]["nixpkgs"]["locked"]["lastModified"] = json!(1);
        let nix = r#"{ pkgs, ... }: {
  cachix = { enable = false; };
  packages = [ pkgs.bash ];
  env = { UPDATE_REVISION = builtins.toString ((builtins.fromJSON (builtins.readFile ./devenv.lock)).nodes.nixpkgs.locked.lastModified); };
  enterTest = ''
    test "$UPDATE_REVISION" != 1
    printf excluded > generated-file
  '';
}
"#;
        let source_commit = api(
            self.repository,
            "GET",
            &format!("git/commits/{source}"),
            None,
        )?;
        let tree = api(
            self.repository,
            "POST",
            "git/trees",
            Some(json!({"base_tree":source_commit["tree"]["sha"],"tree":[
                {"path":"devenv.nix","mode":"100644","type":"blob","content":nix},
                {"path":"devenv.yaml","mode":"100644","type":"blob","content":fs::read_to_string(fixture.join("devenv.yaml"))?},
                {"path":"devenv.lock","mode":"100644","type":"blob","content":serde_json::to_string_pretty(&lock)?}
            ]})),
        )?;
        let commit = api(
            self.repository,
            "POST",
            "git/commits",
            Some(
                json!({"message":"test: Prepare Isolated Update Fixture","tree":tree["sha"],"parents":[source]}),
            ),
        )?;
        api(
            self.repository,
            "POST",
            "git/refs",
            Some(json!({"ref":format!("refs/heads/{base}"),"sha":commit["sha"]})),
        )?;
        write_output("base", &self.base)?;
        write_output("branch", &self.branch)?;
        Ok(())
    }

    fn advance(&self) -> Result<()> {
        let base = &self.base;
        let reference = api(
            self.repository,
            "GET",
            &format!("git/ref/heads/{base}"),
            None,
        )?;
        let parent = reference["object"]["sha"]
            .as_str()
            .context("fixture base SHA")?;
        let commit = api(
            self.repository,
            "GET",
            &format!("git/commits/{parent}"),
            None,
        )?;
        let mut manifest: Value = serde_norway::from_str(&fs::read_to_string(
            self.root.join("tests/fixtures/devenv-update/devenv.yaml"),
        )?)?;
        manifest["inputs"]["fixture"] = json!({"url":"github:NixOS/flake-compat/5edf11c44bc78a0d334f6334cdaf7d60d732daab","flake":false});
        let tree = api(
            self.repository,
            "POST",
            "git/trees",
            Some(json!({"base_tree":commit["tree"]["sha"],"tree":[
                {"path":"devenv.yaml","mode":"100644","type":"blob","content":format!("---\n{}",serde_norway::to_string(&manifest)?)}
            ]})),
        )?;
        let commit = api(
            self.repository,
            "POST",
            "git/commits",
            Some(
                json!({"message":"test: Add a Dependency to the Update Fixture","tree":tree["sha"],"parents":[parent]}),
            ),
        )?;
        api(
            self.repository,
            "PATCH",
            &format!("git/refs/heads/{base}"),
            Some(json!({"sha":commit["sha"]})),
        )?;
        Ok(())
    }

    fn closed(&self) -> Result<()> {
        let prs = self.prs()?;
        let prs = prs.as_array().context("PR list")?;
        ensure!(
            prs.len() == 1 && prs[0]["state"] == "closed",
            "obsolete PR must close"
        );
        Ok(())
    }

    fn cleanup(&self) -> Result<()> {
        for pr in self.prs()?.as_array().context("PR list")? {
            if pr["state"] == "open" {
                api(
                    self.repository,
                    "PATCH",
                    &format!("pulls/{}", pr["number"]),
                    Some(json!({"state":"closed"})),
                )?;
            }
        }
        for name in [&self.branch, &self.base] {
            let refs = api(
                self.repository,
                "GET",
                &format!("git/matching-refs/heads/{name}"),
                None,
            )?;
            if refs
                .as_array()
                .context("refs")?
                .iter()
                .any(|r| r["ref"] == format!("refs/heads/{name}"))
            {
                api(
                    self.repository,
                    "DELETE",
                    &format!("git/refs/heads/{name}"),
                    None,
                )?;
            }
        }
        Ok(())
    }

    fn managed_pr(&self) -> Result<(u64, String, Value)> {
        let prs = self.prs()?;
        let prs = prs.as_array().context("PR list")?;
        ensure!(
            prs.len() == 1,
            "expected one managed PR, found {}",
            prs.len()
        );
        let pr = &prs[0];
        ensure!(pr["state"] == "open", "expected open PR");
        let number = pr["number"].as_u64().context("PR number")?;
        let files = api(
            self.repository,
            "GET",
            &format!("pulls/{number}/files"),
            None,
        )?;
        let files = files.as_array().context("PR files")?;
        ensure!(
            files.len() == 1 && files[0]["filename"] == "devenv.lock",
            "unexpected PR files: {files:?}"
        );
        let sha = pr["head"]["sha"].as_str().context("PR SHA")?;
        let commit = api(self.repository, "GET", &format!("commits/{sha}"), None)?;
        ensure!(
            commit["commit"]["verification"]["verified"] == true,
            "update commit must be signed"
        );
        Ok((number, sha.to_owned(), commit))
    }

    fn verify(&self) -> Result<()> {
        let (number, sha, _) = self.managed_pr()?;
        let mut triggered = false;
        for _ in 0..24 {
            let runs = api(
                self.repository,
                "GET",
                &format!("actions/runs?head_sha={sha}&event=pull_request"),
                None,
            )?;
            if successful_probe(&runs)? {
                triggered = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
        ensure!(
            triggered,
            "App PR did not produce a successful downstream probe"
        );
        write_output("pr-number", &number.to_string())?;
        Ok(())
    }

    fn reconcile(&self) -> Result<()> {
        let base = &self.base;
        let (_, _, commit) = self.managed_pr()?;
        let base_ref = api(
            self.repository,
            "GET",
            &format!("git/ref/heads/{base}"),
            None,
        )?;
        let resolved = api(
            self.repository,
            "POST",
            "git/commits",
            Some(json!({
                "message":"test: Resolve Update on the Fixture Base",
                "tree":commit["commit"]["tree"]["sha"],"parents":[base_ref["object"]["sha"]]
            })),
        )?;
        api(
            self.repository,
            "PATCH",
            &format!("git/refs/heads/{base}"),
            Some(json!({"sha":resolved["sha"]})),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_successful_app_pr_probe_after_workflow_rename() {
        let runs = json!({"workflow_runs": [{
            "name": "Test: Devenv Update PR Probe",
            "path": ".github/workflows/test-devenv-update-probe.yaml",
            "conclusion": "success"
        }]});

        assert!(successful_probe(&runs).unwrap());

        let unrelated = json!({"workflow_runs": [{
            "path": ".github/workflows/repository-ci.yaml",
            "conclusion": "success"
        }]});
        assert!(!successful_probe(&unrelated).unwrap());

        let failed = json!({"workflow_runs": [{
            "path": ".github/workflows/test-devenv-update-probe.yaml",
            "conclusion": "failure"
        }]});
        assert!(!successful_probe(&failed).unwrap());
    }
}
