use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    env, fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

fn api(repo: &str, path: &str, method: &str, body: Option<Value>) -> Result<Value> {
    let mut command = Command::new("gh");
    let endpoint = format!("repos/{repo}/{path}");
    command.args(["api", "--method", method, endpoint.trim_end_matches('/')]);
    if body.is_some() {
        command.args(["--input", "-"]);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    if let Some(body) = body {
        child
            .stdin
            .take()
            .context("API input")?
            .write_all(&serde_json::to_vec(&body)?)?;
    }
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "GitHub fixture API {method} {path}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    if output.stdout.is_empty() {
        Ok(Value::Null)
    } else {
        Ok(serde_json::from_slice(&output.stdout)?)
    }
}

fn merge_method(repository: &Value) -> Result<&'static str> {
    [
        ("allow_squash_merge", "squash"),
        ("allow_merge_commit", "merge"),
        ("allow_rebase_merge", "rebase"),
    ]
    .into_iter()
    .find_map(|(setting, method)| (repository[setting] == true).then_some(method))
    .context("repository has no supported PR merge method enabled")
}

pub(crate) fn run(root: &Path) -> Result<()> {
    let repo = env::var("GITHUB_REPOSITORY")?;
    let run = env::var("GITHUB_RUN_ID")?;
    let attempt = env::var("GITHUB_RUN_ATTEMPT")?;
    let event: Value = serde_json::from_slice(&fs::read(env::var("GITHUB_EVENT_PATH")?)?)?;
    ensure!(
        env::var("CI")? == "true"
            && env::var("GITHUB_EVENT_NAME")? == "push"
            && env::var("GITHUB_REF")?
                == format!(
                    "refs/heads/{}",
                    event["repository"]["default_branch"]
                        .as_str()
                        .context("default branch")?
                ),
        "real lifecycle tests require a trusted default-branch push"
    );
    ensure!(
        run.bytes().all(|b| b.is_ascii_digit()) && attempt.bytes().all(|b| b.is_ascii_digit()),
        "invalid fixture identity"
    );
    let method = merge_method(&api(&repo, "", "GET", None)?)?;
    println!("Cargo CRAP fixture merge method: {method}");
    let baseline_branch = format!("tact-update-base-crap-{run}-{attempt}");
    let records_branch = format!("tact-crap-records-{run}-{attempt}");
    let scratch = root.join(".tars/scratch/crap-lifecycle");
    fs::create_dir_all(&scratch)?;
    let fixture = tempfile::tempdir_in(scratch)?;
    let source = env::var("GITHUB_SHA")?;
    let parent = api(&repo, &format!("git/commits/{source}"), "GET", None)?;
    let seed = api(
        &repo,
        "git/commits",
        "POST",
        Some(
            json!({"message":"test: seed CRAP lifecycle fixture","tree":parent["tree"]["sha"],"parents":[source]}),
        ),
    )?;
    let mut revision = seed["sha"].as_str().context("fixture commit")?.to_owned();
    api(
        &repo,
        "git/refs",
        "POST",
        Some(json!({"ref":format!("refs/heads/{baseline_branch}"),"sha":revision})),
    )?;
    let mut pr_number = None;
    let result = (|| -> Result<()> {
        let invoke = |revision: &str, number: u64| -> Result<Option<Value>> {
            // The publisher validates provenance and hashes; real analysis and schema validation have separate native tests.
            fs::write(
                fixture.path().join("baseline.json"),
                format!("{{\"version\":\"0.6.1\",\"entries\":[],\"fixture\":{number}}}\n"),
            )?;
            fs::write(
                fixture.path().join("crap-badge.json"),
                format!(
                    "{{\"schemaVersion\":1,\"label\":\"CRAP fixture\",\"message\":\"{number}\",\"color\":\"green\"}}\n"
                ),
            )?;
            let digest = |name: &str| -> Result<String> {
                let output = Command::new("sha256sum")
                    .arg(fixture.path().join(name))
                    .output()?;
                ensure!(output.status.success(), "fixture hash");
                Ok(String::from_utf8(output.stdout)?
                    .split_whitespace()
                    .next()
                    .context("hash")?
                    .to_owned())
            };
            fs::write(
                fixture.path().join("metadata.json"),
                serde_json::to_vec(
                    &json!({"complete":true,"operation":"measure","commit":revision,"run_id":run,"baseline_hash":digest("baseline.json")?,"badge_hash":digest("crap-badge.json")?}),
                )?,
            )?;
            fs::write(
                fixture.path().join("event.json"),
                serde_json::to_vec(&json!({"repository":{"default_branch":baseline_branch}}))?,
            )?;
            let outputs = fixture.path().join("output");
            fs::write(&outputs, "")?;
            let status = Command::new("node")
                .arg(root.join("composite/cargo-crap/scripts/github/main.mjs"))
                .env("INPUT_PHASE", "record")
                .env("INPUT_TOKEN", env::var("GH_TOKEN")?)
                .env("INPUT_BASELINE-BRANCH", &baseline_branch)
                .env("INPUT_RECORDS-BRANCH", &records_branch)
                .env("INPUT_REPORT-DIRECTORY", fixture.path())
                .env("GITHUB_OUTPUT", &outputs)
                .env("GITHUB_SHA", revision)
                .env("GITHUB_REF", format!("refs/heads/{baseline_branch}"))
                .env("GITHUB_EVENT_NAME", "push")
                .env("GITHUB_EVENT_PATH", fixture.path().join("event.json"))
                .status()?;
            ensure!(status.success(), "real managed publisher failed");
            let prs = api(
                &repo,
                &format!(
                    "pulls?state=open&head={}:{}",
                    repo.split('/').next().context("owner")?,
                    records_branch
                ),
                "GET",
                None,
            )?;
            let prs = prs.as_array().context("open PRs")?;
            ensure!(prs.len() <= 1, "more than one CRAP fixture PR");
            Ok(prs.first().cloned())
        };
        let pr = invoke(&revision, 1)?.context("first recording PR")?;
        let number = pr["number"].as_u64().context("PR number")?;
        pr_number = Some(number);
        // Several source merges update the same waiting PR.
        for measurement in 2..=10 {
            let next = api(
                &repo,
                "git/commits",
                "POST",
                Some(
                    json!({"message":format!("test: advance CRAP fixture {measurement}"),"tree":parent["tree"]["sha"],"parents":[revision]}),
                ),
            )?;
            revision = next["sha"].as_str().context("advance commit")?.into();
            api(
                &repo,
                &format!("git/refs/heads/{baseline_branch}"),
                "PATCH",
                Some(json!({"sha":revision,"force":false})),
            )?;
            ensure!(
                invoke(&revision, measurement)?.context("updated PR")?["number"] == number,
                "must refresh the same PR"
            );
        }
        let files = api(&repo, &format!("pulls/{number}/files"), "GET", None)?;
        let files = files.as_array().context("record files")?;
        ensure!(
            files.len() == 2
                && files.iter().all(|f| [
                    ".github/crap/baseline.json",
                    ".github/badges/crap-badge.json"
                ]
                .contains(&f["filename"].as_str().unwrap_or(""))),
            "recording allowlist"
        );
        let merged = api(
            &repo,
            &format!("pulls/{number}/merge"),
            "PUT",
            Some(json!({"merge_method":method})),
        )?;
        ensure!(merged["merged"] == true, "fixture recording merge");
        revision = merged["sha"].as_str().context("merged commit")?.into();
        ensure!(
            invoke(&revision, 10)?.is_none(),
            "recording merge must not create another PR"
        );
        println!("PASS real Cargo CRAP managed PR lifecycle: create, refresh, merge and no-op");
        Ok(())
    })();
    if let Some(number) = pr_number {
        let _ = api(
            &repo,
            &format!("pulls/{number}"),
            "PATCH",
            Some(json!({"state":"closed"})),
        );
    }
    for branch in [&records_branch, &baseline_branch] {
        let _ = api(&repo, &format!("git/refs/heads/{branch}"), "DELETE", None);
    }
    result
}
