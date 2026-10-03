use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

const COMMIT: &str = "1111111111111111111111111111111111111111";
const BASE: &str = "2222222222222222222222222222222222222222";
const HEAD: &str = "3333333333333333333333333333333333333333";
const MARKER: &str = "<!-- tars-cloud/actions:cargo-crap:v1 -->";

pub(crate) fn contracts(root: &Path) -> Result<()> {
    let workflow = super::workflows::load(root, ".github/workflows/consumer-cargo-crap.yaml")?;
    let jobs = &workflow["jobs"];
    ensure!(
        jobs["analyze"]["permissions"]["contents"] == "read"
            && jobs["analyze"]["permissions"]["actions"] == "read",
        "analysis must keep read-only credentials"
    );
    let gate = super::workflows::step(&workflow, "analyze", "enforce_quality")?;
    ensure!(
        gate["run"] == "test \"$QUALITY\" = pass"
            && gate["if"]
                .as_str()
                .context("quality condition")?
                .contains("complete == 'true'"),
        "quality must be enforced after completed reports"
    );
    let upload = super::workflows::step(&workflow, "analyze", "upload_reports")?;
    ensure!(
        upload["if"]
            .as_str()
            .context("artifact condition")?
            .contains("complete == 'true'")
            && upload["with"]["path"]
                .as_str()
                .context("artifact paths")?
                .contains("/metadata.json"),
        "upload only completed reports with provenance"
    );
    for name in ["comment", "record"] {
        let steps = jobs[name]["steps"].as_array().context("publisher steps")?;
        ensure!(
            steps.iter().all(|step| step.get("run").is_none()
                && !step["uses"].as_str().unwrap_or("").contains("checkout")),
            "publishers must not execute consumer code or enter its environment"
        );
    }
    ensure!(
        super::workflows::step(&workflow, "comment", "download_reports")?["continue-on-error"]
            == true
            && super::workflows::step(&workflow, "comment", "publish_comment")?["continue-on-error"]
                == true,
        "optional comment publication defaults to warnings"
    );
    let token = super::workflows::step(&workflow, "record", "create_app_token")?;
    ensure!(
        token["with"]["permission-contents"] == "write"
            && token["with"]["permission-pull-requests"] == "write"
            && token["with"]["repositories"].is_string(),
        "record token must be repository scoped"
    );
    let tests = super::workflows::load(root, ".github/workflows/test-cargo-crap.yaml")?;
    let matrix = &tests["jobs"]["native"]["strategy"]["matrix"];
    ensure!(
        matrix["runner"] == json!(["ubuntu-24.04", "ubuntu-24.04-arm"])
            && matrix["type"] == json!(["devenv", "flakes"])
            && matrix["backend"] == json!(["llvm-cov", "tarpaulin"]),
        "all eight native coverage profiles must be tested"
    );
    ensure!(
        tests["jobs"]["consumer"]["uses"] == "./.github/workflows/consumer-cargo-crap.yaml",
        "real reusable caller must consume the revision under test"
    );
    ensure!(
        tests["jobs"]["consumer"]["permissions"]["pull-requests"] == "write",
        "nested caller must permit the optional comment job's declared permission"
    );
    Ok(())
}

struct Response {
    method: &'static str,
    path: String,
    status: u16,
    body: Value,
}

fn response(method: &'static str, path: &str, body: Value) -> Response {
    Response {
        method,
        path: path.into(),
        status: 200,
        body,
    }
}

fn invoke(
    root: &Path,
    phase: &str,
    event_name: &str,
    event: Value,
    responses: Vec<Response>,
    report: Option<&Path>,
) -> Result<(bool, String)> {
    let scratch = root.join(".tars/scratch/crap-api");
    fs::create_dir_all(&scratch)?;
    let fixture = tempfile::tempdir_in(scratch)?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> Result<()> {
        for expected in responses {
            let started = Instant::now();
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        ensure!(
                            started.elapsed() < Duration::from_secs(10),
                            "missing request {} {}",
                            expected.method,
                            expected.path
                        );
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => return Err(e.into()),
                }
            };
            stream.set_read_timeout(Some(Duration::from_secs(5)))?;
            let mut bytes = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let read = stream.read(&mut chunk)?;
                ensure!(read > 0, "incomplete request");
                bytes.extend_from_slice(&chunk[..read]);
                if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|v| v.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let text = String::from_utf8(bytes)?;
            let first = text.lines().next().context("request line")?;
            ensure!(
                first.starts_with(&format!(
                    "{} /repos/example/project/{}",
                    expected.method, expected.path
                )),
                "unexpected request {first}; wanted {} {}",
                expected.method,
                expected.path
            );
            if expected.path == "git/trees" && expected.method == "POST" {
                let body: Value =
                    serde_json::from_str(text.split_once("\r\n\r\n").context("body")?.1)?;
                let paths: Vec<_> = body["tree"]
                    .as_array()
                    .context("tree")?
                    .iter()
                    .map(|e| e["path"].as_str().unwrap())
                    .collect();
                ensure!(
                    paths
                        == [
                            ".github/crap/baseline.json",
                            ".github/badges/crap-badge.json"
                        ],
                    "record tree allowlist"
                );
            }
            if expected.path == "git/refs/heads/crap/next" {
                let body: Value =
                    serde_json::from_str(text.split_once("\r\n\r\n").context("body")?.1)?;
                ensure!(
                    body["force"] == false,
                    "managed branch update must reject concurrent changes"
                );
            }
            let body = serde_json::to_string(&expected.body)?;
            write!(
                stream,
                "HTTP/1.1 {} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                expected.status,
                body.len(),
                body
            )?;
        }
        Ok(())
    });
    let event_path = fixture.path().join("event.json");
    let output_path = fixture.path().join("output");
    fs::write(&event_path, serde_json::to_vec(&event)?)?;
    fs::write(&output_path, "")?;
    let output = Command::new(crate::runner::executable("node")?)
        .arg(root.join("composite/cargo-crap/scripts/github/main.mjs"))
        .env_clear()
        .env("INPUT_PHASE", phase)
        .env("INPUT_TOKEN", "fixture-token")
        .env("INPUT_BASELINE-COMMIT", BASE)
        .env("INPUT_REPORT-DIRECTORY", report.unwrap_or(Path::new("")))
        .env("GITHUB_EVENT_PATH", event_path)
        .env("GITHUB_EVENT_NAME", event_name)
        .env("GITHUB_REPOSITORY", "example/project")
        .env("GITHUB_SHA", COMMIT)
        .env("GITHUB_REF", "refs/heads/trunk")
        .env("GITHUB_RUN_ID", "42")
        .env("GITHUB_API_URL", format!("http://{address}"))
        .env("GITHUB_SERVER_URL", "https://github.example.invalid")
        .env("GITHUB_OUTPUT", &output_path)
        .output()?;
    server
        .join()
        .map_err(|_| anyhow::anyhow!("API fixture panicked"))??;
    Ok((
        output.status.success(),
        format!(
            "{}{}{}",
            fs::read_to_string(output_path)?,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    ))
}

pub(crate) fn github(root: &Path) -> Result<()> {
    let event = json!({"repository":{"default_branch":"trunk"},"number":7,"pull_request":{"base":{"ref":"trunk","sha":BASE},"head":{"sha":HEAD}}});
    let merge = json!({"parents":[{"sha":BASE},{"sha":HEAD}]});
    let (success, output) = invoke(
        root,
        "resolve",
        "pull_request",
        event.clone(),
        vec![response("GET", &format!("git/commits/{COMMIT}"), merge)],
        None,
    )?;
    ensure!(
        success
            && output.contains(&format!("baseline-commit={BASE}"))
            && output.contains("operation=compare"),
        "exact proposed merge resolution: {output}"
    );
    let (success, _) = invoke(
        root,
        "resolve",
        "pull_request",
        event.clone(),
        vec![response(
            "GET",
            &format!("git/commits/{COMMIT}"),
            json!({"parents":[{"sha":COMMIT},{"sha":HEAD}]}),
        )],
        None,
    )?;
    ensure!(!success, "wrong baseline parent must fail");
    let (success, _) = invoke(
        root,
        "resolve",
        "pull_request_target",
        event.clone(),
        vec![],
        None,
    )?;
    ensure!(!success, "privileged PR execution must fail");
    let (success, output) = invoke(
        root,
        "find-baseline",
        "pull_request",
        event.clone(),
        vec![
            response("GET", "actions/runs/42", json!({"workflow_id":9})),
            response(
                "GET",
                "actions/workflows/9/runs?",
                json!({"workflow_runs":[{"id":10,"head_sha":BASE,"head_branch":"trunk","event":"pull_request","repository":{"full_name":"example/project"}},{"id":11,"head_sha":COMMIT,"head_branch":"trunk","event":"push","repository":{"full_name":"example/project"}},{"id":12,"head_sha":BASE,"head_branch":"trunk","event":"push","repository":{"full_name":"example/project"}}]}),
            ),
            response(
                "GET",
                "actions/runs/12/artifacts?",
                json!({"artifacts":[{"id":70,"name":"cargo-crap-default","expired":true},{"id":71,"name":"cargo-crap-default","expired":false}]}),
            ),
        ],
        None,
    )?;
    ensure!(
        success && output.contains("artifact-id=71"),
        "trusted exact baseline lookup: {output}"
    );
    let scratch = root.join(".tars/scratch/crap-records");
    fs::create_dir_all(&scratch)?;
    let reports = tempfile::tempdir_in(scratch)?;
    let metadata = json!({"complete":true,"operation":"compare","run_id":"42","commit":COMMIT,"head_commit":HEAD,"baseline_commit":BASE});
    fs::write(
        reports.path().join("metadata.json"),
        serde_json::to_vec(&metadata)?,
    )?;
    let (success, output) = invoke(
        root,
        "comment",
        "pull_request",
        event.clone(),
        vec![response(
            "GET",
            "pulls/7",
            json!({"state":"open","head":{"sha":COMMIT}}),
        )],
        Some(reports.path()),
    )?;
    ensure!(
        success && output.contains("Superseded"),
        "stale comment skip: {output}"
    );
    let (success, output) = invoke(
        root,
        "comment",
        "pull_request",
        event.clone(),
        vec![response(
            "GET",
            "pulls/7",
            json!({"state":"open","head":{"sha":HEAD},"base":{"sha":COMMIT}}),
        )],
        Some(reports.path()),
    )?;
    ensure!(
        success && output.contains("Superseded"),
        "unchanged topic head with an advanced baseline must not overwrite a newer comment"
    );
    fs::write(
        reports.path().join("summary.md"),
        "CRAP quality: **fail**.\n",
    )?;
    let (success, _) = invoke(
        root,
        "comment",
        "pull_request",
        event.clone(),
        vec![
            response(
                "GET",
                "pulls/7",
                json!({"state":"open","head":{"sha":HEAD},"base":{"sha":BASE}}),
            ),
            response(
                "GET",
                "issues/7/comments?",
                json!([{"id":9,"user":{"type":"User","login":"github-actions[bot]"},"body":format!("{MARKER}\n<!-- analysis:default -->")} ]),
            ),
            response("POST", "issues/7/comments", json!({"id":10})),
        ],
        Some(reports.path()),
    )?;
    ensure!(success, "a human's copied marker must not be overwritten");
    fs::write(
        reports.path().join("metadata.json"),
        serde_json::to_vec(
            &json!({"complete":true,"operation":"measure","run_id":"42","commit":COMMIT}),
        )?,
    )?;
    let (success, output) = invoke(
        root,
        "record",
        "push",
        event.clone(),
        vec![response(
            "GET",
            "git/ref/heads/trunk",
            json!({"object":{"sha":BASE}}),
        )],
        Some(reports.path()),
    )?;
    ensure!(
        success && output.contains("stale"),
        "stale measurement skip"
    );
    let (success, _) = invoke(
        root,
        "record",
        "push",
        event,
        vec![
            response(
                "GET",
                "git/ref/heads/trunk",
                json!({"object":{"sha":COMMIT}}),
            ),
            response(
                "GET",
                "pulls?state=open",
                json!([{"base":{"ref":"trunk"},"head":{"repo":{"full_name":"example/project"}},"user":{"type":"User"},"body":MARKER}]),
            ),
            Response {
                method: "GET",
                path: "git/ref/heads/crap%2Fnext".into(),
                status: 404,
                body: json!({}),
            },
        ],
        Some(reports.path()),
    )?;
    ensure!(!success, "unowned recording PR must fail");
    for name in ["baseline.json", "crap-badge.json"] {
        fs::write(reports.path().join(name), "{}\n")?;
    }
    let digest = |name: &str| -> Result<String> {
        let output = Command::new("sha256sum")
            .arg(reports.path().join(name))
            .output()?;
        ensure!(
            output.status.success(),
            "record fixture hash: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8(output.stdout)?
            .split_whitespace()
            .next()
            .context("record hash")?
            .into())
    };
    fs::write(
        reports.path().join("metadata.json"),
        serde_json::to_vec(
            &json!({"complete":true,"operation":"measure","run_id":"42","commit":COMMIT,"baseline_hash":digest("baseline.json")?,"badge_hash":digest("crap-badge.json")?}),
        )?,
    )?;
    let producer = json!({"repository":{"default_branch":"trunk"}});
    let missing = |path: &str| Response {
        method: "GET",
        path: path.into(),
        status: 404,
        body: json!({}),
    };
    let (success, output) = invoke(
        root,
        "record",
        "push",
        producer.clone(),
        vec![
            response(
                "GET",
                "git/ref/heads/trunk",
                json!({"object":{"sha":COMMIT}}),
            ),
            response("GET", "pulls?state=open", json!([])),
            missing("git/ref/heads/crap%2Fnext"),
            missing("contents/.github/crap/baseline.json?"),
            missing("contents/.github/badges/crap-badge.json?"),
            response(
                "GET",
                &format!("git/commits/{COMMIT}"),
                json!({"tree":{"sha":BASE}}),
            ),
            response("POST", "git/trees", json!({"sha":BASE})),
            response("POST", "git/commits", json!({"sha":HEAD})),
            response(
                "GET",
                "git/ref/heads/trunk",
                json!({"object":{"sha":COMMIT}}),
            ),
            response("POST", "git/refs", json!({})),
            response(
                "POST",
                "pulls",
                json!({"number":8,"html_url":"https://github.example.invalid/example/project/pull/8"}),
            ),
        ],
        Some(reports.path()),
    )?;
    ensure!(
        success && output.contains("pr-url="),
        "first recording PR creation: {output}"
    );
    let owned = json!({"number":8,"base":{"ref":"trunk"},"head":{"sha":HEAD,"repo":{"full_name":"example/project"}},"user":{"type":"Bot","login":"app[bot]"},"body":MARKER});
    let prefix = || {
        vec![
            response(
                "GET",
                "git/ref/heads/trunk",
                json!({"object":{"sha":COMMIT}}),
            ),
            response("GET", "pulls?state=open", json!([owned.clone()])),
            response(
                "GET",
                "git/ref/heads/crap%2Fnext",
                json!({"object":{"sha":HEAD}}),
            ),
            response(
                "GET",
                "pulls/8/files?",
                json!([{"filename":".github/crap/baseline.json"},{"filename":".github/badges/crap-badge.json"}]),
            ),
            response(
                "GET",
                &format!("commits/{HEAD}"),
                json!({"commit":{"message":MARKER},"committer":{"type":"Bot","login":"app[bot]"}}),
            ),
        ]
    };
    let mut refresh = prefix();
    refresh.extend([
        missing("contents/.github/crap/baseline.json?"),
        missing("contents/.github/badges/crap-badge.json?"),
        response(
            "GET",
            &format!("git/commits/{COMMIT}"),
            json!({"tree":{"sha":BASE}}),
        ),
        response("POST", "git/trees", json!({"sha":BASE})),
        response("POST", "git/commits", json!({"sha":HEAD})),
        response(
            "GET",
            "git/ref/heads/trunk",
            json!({"object":{"sha":COMMIT}}),
        ),
        response("PATCH", "git/refs/heads/crap/next", json!({})),
        response(
            "PATCH",
            "pulls/8",
            json!({"number":8,"html_url":"https://github.example.invalid/example/project/pull/8"}),
        ),
    ]);
    let (success, output) = invoke(
        root,
        "record",
        "push",
        producer.clone(),
        refresh,
        Some(reports.path()),
    )?;
    ensure!(
        success && output.contains("pr-url="),
        "refresh one managed PR with guarded branch update: {output}"
    );
    let mut noop = prefix();
    noop.extend([
        response(
            "GET",
            "contents/.github/crap/baseline.json?",
            json!({"type":"file","sha":"0967ef424bce6791893e9a57bb952f80fd536e93","encoding":"none","content":""}),
        ),
        response(
            "GET",
            "contents/.github/badges/crap-badge.json?",
            json!({"type":"file","sha":"0967ef424bce6791893e9a57bb952f80fd536e93","encoding":"base64","content":"e30K"}),
        ),
        response("PATCH", "pulls/8", json!({"state":"closed"})),
    ]);
    let (success, output) = invoke(root, "record", "push", producer, noop, Some(reports.path()))?;
    ensure!(
        success && output.contains("already match"),
        "identical records close the obsolete PR without creating another: {output}"
    );
    println!(
        "PASS Cargo CRAP GitHub boundaries: merge parents, trusted artifacts, stale publishers and ownership"
    );
    Ok(())
}

fn git(directory: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(directory)
        .output()?;
    ensure!(
        output.status.success(),
        "fixture git: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().into())
}

fn copy(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            copy(&entry.path(), &to.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), to.join(entry.file_name()))?;
        }
    }
    Ok(())
}

pub(crate) fn native(root: &Path, backend: &str, environment: &str) -> Result<()> {
    ensure!(
        matches!(backend, "llvm-cov" | "tarpaulin") && matches!(environment, "devenv" | "flakes"),
        "invalid native fixture profile"
    );
    let scratch = root.join(".tars/scratch/crap-native");
    fs::create_dir_all(&scratch)?;
    let fixture = tempfile::tempdir_in(scratch)?;
    let project = fixture.path().join("project");
    copy(&root.join("tests/fixtures/cargo-crap"), &project)?;
    let module = fs::read_to_string(project.join("devenv.nix"))?;
    fs::write(
        project.join("devenv.nix"),
        module.replace(
            "source = ../../..;",
            &format!(
                "source = builtins.toPath {};",
                serde_json::to_string(root.to_str().context("fixture source path")?)?
            ),
        ),
    )?;
    fs::write(
        project.join(".cargo-crap.toml"),
        "top = 1\nmin = 1000\nfail-above = true\nfail-regression = true\n",
    )?;
    git(&project, &["init", "-b", "trunk"])?;
    git(&project, &["config", "user.name", "Tact Fixture"])?;
    git(&project, &["config", "user.email", "tact@example.invalid"])?;
    git(&project, &["add", "."])?;
    git(&project, &["commit", "-m", "test: baseline"])?;
    let base = git(&project, &["rev-parse", "HEAD"])?;
    let invoke = |operation: &str,
                  revision: &str,
                  baseline_revision: &str,
                  baseline_directory: &Path|
     -> Result<PathBuf> {
        let output = fixture.path().join("output");
        fs::write(&output, "")?;
        let arch = if std::env::consts::ARCH == "aarch64" {
            "ARM64"
        } else {
            "X64"
        };
        let status = Command::new("bash")
            .arg(root.join("composite/cargo-crap/scripts/dispatch.sh"))
            .current_dir(root)
            .env("TARS_ACTIONS_ROOT", root)
            .env("GITHUB_WORKSPACE", &project)
            .env("RUNNER_TEMP", fixture.path())
            .env("RUNNER_OS", "Linux")
            .env("RUNNER_ARCH", arch)
            .env("RUNNER_ENVIRONMENT", "self-hosted")
            .env("GITHUB_OUTPUT", &output)
            .env("ENVIRONMENT_TYPE", environment)
            .env("PROJECT_DIRECTORY", ".")
            .env("ENVIRONMENT_SYSTEM", "")
            .env("FLAKE_SHELL", ".#named")
            .env("CRAP_ACTION_ROOT", root.join("composite/cargo-crap"))
            .env("CRAP_OPERATION", operation)
            .env("CRAP_COMMIT", revision)
            .env("CRAP_BASELINE_COMMIT", baseline_revision)
            .env("CRAP_BASELINE_DIRECTORY", baseline_directory)
            .env("CRAP_COVERAGE_TOOL", backend)
            .env("CRAP_PACKAGES", "[\"crap-fixture\"]")
            .env("CRAP_FEATURES", "[]")
            .env_remove("CRAP_MANIFEST_DIRECTORY")
            .env_remove("LLVM_COV")
            .env_remove("LLVM_PROFDATA")
            .env_remove("GITHUB_STEP_SUMMARY")
            .status()?;
        ensure!(
            status.success(),
            "native {environment}/{backend} action failed"
        );
        let outputs = crate::output::values(&fs::read_to_string(output)?)?;
        Ok(PathBuf::from(
            outputs
                .get("report-directory")
                .context("native report directory")?,
        ))
    };
    let baseline = invoke("measure", &base, &base, Path::new(""))?;
    let source = fs::read_to_string(project.join("src/lib.rs"))?;
    fs::write(
        project.join("src/lib.rs"),
        source.replace("assert_eq!(super::choose(false), 2);", "")
            + r#"

pub fn uncovered(value: u8) -> u8 {
    if value == 1 { 1 } else if value == 2 { 2 } else if value == 3 { 3 }
    else if value == 4 { 4 } else if value == 5 { 5 } else { 0 }
}
"#,
    )?;
    git(&project, &["add", "src/lib.rs"])?;
    git(&project, &["commit", "-m", "test: regress coverage"])?;
    let head = git(&project, &["rev-parse", "HEAD"])?;
    let comparison = invoke("compare", &head, &base, &baseline)?;
    let result: Value = serde_json::from_slice(&fs::read(comparison.join("result.json"))?)?;
    ensure!(
        result["quality"] == "fail"
            && result["regressed"].as_u64().unwrap_or(0) > 0
            && result["new_above"] == 1
            && result["baseline_source"] == "artifact",
        "native regression gate: {result}"
    );
    let fallback = invoke("compare", &head, &base, Path::new(""))?;
    let fallback: Value = serde_json::from_slice(&fs::read(fallback.join("result.json"))?)?;
    ensure!(
        fallback["quality"] == "fail" && fallback["baseline_source"] == "fresh",
        "native fresh baseline fallback"
    );
    let accepted = invoke("measure", &head, &base, Path::new(""))?;
    let result: Value = serde_json::from_slice(&fs::read(accepted.join("result.json"))?)?;
    ensure!(
        result["quality"] == "pass" && result["existing_debt"].as_u64().unwrap_or(0) > 0,
        "accepted baseline debt"
    );
    let next = invoke("compare", &head, &head, &accepted)?;
    let next: Value = serde_json::from_slice(&fs::read(next.join("result.json"))?)?;
    ensure!(
        next["quality"] == "pass" && next["baseline_source"] == "artifact",
        "unchanged accepted debt must pass the next comparison"
    );
    ensure!(
        fs::read_to_string(project.join(".cargo-crap.toml"))?.contains("top = 1"),
        "consumer config must remain untouched"
    );
    println!(
        "PASS Cargo CRAP native {environment}/{backend}: coverage, new functions, regression, artifact reuse, fresh fallback, accepted debt"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn github_boundaries() {
        super::github(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .canonicalize()
                .unwrap(),
        )
        .unwrap();
    }
}
