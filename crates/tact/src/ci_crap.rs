use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
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

fn tree_files(repo: &str, tree_sha: &str) -> Result<BTreeSet<String>> {
    let tree = api(
        repo,
        &format!("git/trees/{tree_sha}?recursive=1"),
        "GET",
        None,
    )?;
    ensure!(
        tree["truncated"] != true,
        "fixture tree unexpectedly truncated"
    );
    Ok(tree["tree"]
        .as_array()
        .context("fixture tree entries")?
        .iter()
        .filter(|entry| entry["type"] == "blob")
        .filter_map(|entry| entry["path"].as_str().map(str::to_owned))
        .collect())
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
    let result = (|| -> Result<()> {
        let invoke = |revision: &str, analysis_id: &str, number: u64| -> Result<(String, String)> {
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
                .env("INPUT_ANALYSIS-ID", analysis_id)
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
            let values = crate::output::values(&fs::read_to_string(&outputs)?)?;
            Ok((
                values.get("records-branch").cloned().unwrap_or_default(),
                values.get("records-commit").cloned().unwrap_or_default(),
            ))
        };

        // 1. First publication creates an orphan branch with root baseline and badge files.
        let (branch1, commit1) = invoke(&revision, "default", 1)?;
        ensure!(
            branch1 == records_branch && !commit1.is_empty(),
            "first orphan publication must return records branch and commit"
        );
        let head1 = api(&repo, &format!("git/commits/{commit1}"), "GET", None)?;
        ensure!(
            head1["parents"]
                .as_array()
                .context("orphan parents")?
                .is_empty(),
            "first CRAP records commit must be an orphan commit with zero parents"
        );
        let files1 = tree_files(&repo, head1["tree"]["sha"].as_str().context("tree 1")?)?;
        ensure!(
            files1 == BTreeSet::from(["baseline.json".into(), "crap-badge.json".into()]),
            "unexpected initial orphan branch files: {files1:?}"
        );

        // 2. Subsequent measurement advances the orphan branch fast-forward.
        let next = api(
            &repo,
            "git/commits",
            "POST",
            Some(
                json!({"message":"test: advance CRAP fixture 2","tree":parent["tree"]["sha"],"parents":[revision]}),
            ),
        )?;
        revision = next["sha"].as_str().context("advance commit")?.into();
        api(
            &repo,
            &format!("git/refs/heads/{baseline_branch}"),
            "PATCH",
            Some(json!({"sha":revision,"force":false})),
        )?;
        let (branch2, commit2) = invoke(&revision, "default", 2)?;
        ensure!(
            branch2 == records_branch && !commit2.is_empty() && commit2 != commit1,
            "second publication must append a new commit"
        );
        let head2 = api(&repo, &format!("git/commits/{commit2}"), "GET", None)?;
        let parents2: Vec<_> = head2["parents"]
            .as_array()
            .context("parents 2")?
            .iter()
            .filter_map(|p| p["sha"].as_str())
            .collect();
        ensure!(
            parents2 == [commit1.as_str()],
            "second CRAP records commit must fast-forward from the first"
        );

        // 3. A second analysis-id preserves existing default records in the same branch tree.
        let (branch3, commit3) = invoke(&revision, "extra", 3)?;
        ensure!(
            branch3 == records_branch && !commit3.is_empty() && commit3 != commit2,
            "multi-analysis publication must append a new commit"
        );
        let head3 = api(&repo, &format!("git/commits/{commit3}"), "GET", None)?;
        let files3 = tree_files(&repo, head3["tree"]["sha"].as_str().context("tree 3")?)?;
        ensure!(
            files3
                == BTreeSet::from([
                    "baseline.json".into(),
                    "crap-badge.json".into(),
                    "extra/baseline.json".into(),
                    "extra/crap-badge.json".into(),
                ]),
            "multi-analysis update must preserve existing analysis files: {files3:?}"
        );

        // 4. Re-recording identical files is a clean no-op.
        let (branch4, commit4) = invoke(&revision, "extra", 3)?;
        ensure!(
            branch4 == records_branch && commit4.is_empty(),
            "identical CRAP records must no-op without writing a commit"
        );
        println!(
            "PASS real Cargo CRAP orphan branch lifecycle: create, append, multi-analysis preserve and no-op"
        );
        Ok(())
    })();
    for branch in [&records_branch, &baseline_branch] {
        let _ = api(&repo, &format!("git/refs/heads/{branch}"), "DELETE", None);
    }
    result
}
