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
    let composite = super::workflows::load(root, "composite/cargo-crap/action.yaml")?;
    let input = "llvm-profile-merge-failure-mode";
    let variable = "CRAP_LLVM_PROFILE_MERGE_FAILURE_MODE";
    ensure!(
        workflow["on"]["workflow_call"]["inputs"][input]["default"] == "any"
            && workflow["on"]["workflow_call"]["inputs"][input]["type"] == "string"
            && super::workflows::step(&workflow, "analyze", "run_analysis")?["with"][input]
                == format!("${{{{ inputs.{input} }}}}")
            && composite["inputs"][input]["default"] == "any"
            && composite["runs"]["steps"][0]["env"][variable]
                == format!("${{{{ inputs.{input} }}}}"),
        "LLVM merge policy must default to any and reach the analyzer through an environment variable"
    );
    let preflight = fs::read_to_string(root.join("composite/cargo-crap/scripts/analyze.sh"))?;
    ensure!(preflight.contains("\"$CRAP_HELPER\" validate-merge-policy")
        && preflight.contains("cargo llvm-cov --locked --failure-mode \"${CRAP_LLVM_PROFILE_MERGE_FAILURE_MODE:-any}\""),
        "preflight must validate merge overrides before smoke coverage and apply the selected mode");
    let jobs = &workflow["jobs"];
    ensure!(
        workflow["on"]["workflow_call"]["inputs"]["strict-bootstrap"]["default"] == false
            && workflow["on"]["workflow_call"]["inputs"]["strict-bootstrap"]["type"] == "boolean"
            && super::workflows::step(&workflow, "analyze", "run_analysis")?["with"]["strict-bootstrap"]
                == "${{ inputs.strict-bootstrap }}",
        "strict bootstrap must default off and reach the public action"
    );
    ensure!(
        jobs["comment"]["if"]
            .as_str()
            .context("comment condition")?
            .contains("github.event_name == 'pull_request'")
            && jobs["record"]["if"]
                .as_str()
                .context("record condition")?
                .contains("operation == 'measure'")
            && !jobs["record"]["if"].as_str().unwrap().contains("quality"),
        "merge groups must not publish; completed measurements may record failing quality"
    );
    publication_lock(
        &jobs["record"],
        "cargo-crap-records-${{ github.repository }}",
    )?;
    ensure!(
        jobs["analyze"]["permissions"]["contents"] == "read"
            && jobs["analyze"]["permissions"]["actions"] == "read",
        "analysis must keep read-only credentials"
    );
    let gate = super::workflows::step(&workflow, "analyze", "enforce_quality")?;
    ensure!(
        gate["run"]
            .as_str()
            .context("quality script")?
            .trim_end()
            .ends_with("test \"$QUALITY\" = pass")
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
            steps
                .iter()
                .filter(|step| step["id"] != "validate_runner")
                .all(|step| step.get("run").is_none()
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
    let prepare = super::workflows::step(&tests, "native", "prepare_fixture")?;
    let measure = super::workflows::step(&tests, "native", "measure_fixture")?;
    ensure!(
        prepare["run"]
            .as_str()
            .context("fixture preparation")?
            .ends_with("tact integration cargo-crap-fixture")
            && measure["with"]["working-directory"]
                == "${{ steps.prepare_fixture.outputs.directory }}"
            && measure["with"]["commit-sha"] == "${{ steps.prepare_fixture.outputs.commit }}",
        "public coverage must analyze the standalone fixture checkout and its own commit"
    );
    let matrix = &tests["jobs"]["native"]["strategy"]["matrix"];
    ensure!(
        matrix["runner"] == json!(["ubuntu-24.04", "ubuntu-24.04-arm"])
            && matrix["type"] == json!(["devenv", "flakes"])
            && matrix["backend"] == json!(["llvm-cov", "tarpaulin"])
            && matrix["merge-policy"] == json!(["any", "all"])
            && matrix["exclude"] == json!([{"backend":"tarpaulin","merge-policy":"all"}]),
        "native coverage must test both LLVM policies and separate Tarpaulin behavior on both architectures and environments"
    );
    ensure!(
        super::workflows::step(&tests, "native", "measure_fixture")?["with"][input]
            == "${{ matrix.merge-policy }}"
            && super::workflows::step(&tests, "native", "verify_comparison")?["env"][variable]
                == "${{ matrix.merge-policy }}"
            && tests["jobs"]["consumer"]["with"][input] == "all",
        "native public composite and reusable caller must exercise selected merge policies"
    );
    ensure!(
        tests["jobs"]["consumer"]["uses"] == "./.github/workflows/consumer-cargo-crap.yaml",
        "real reusable caller must consume the revision under test"
    );
    ensure!(
        tests["jobs"]["consumer"]["permissions"]["pull-requests"] == "write",
        "nested caller must permit the optional comment job's declared permission"
    );
    recording_contract(root, &tests)
}

fn recording_contract(root: &Path, workflow: &Value) -> Result<()> {
    publication_lock(
        &workflow["jobs"]["records"],
        "cargo-crap-lifecycle-${{ github.repository }}",
    )?;
    let token = super::workflows::step(workflow, "records", "create_app_token")?;
    ensure!(
        token["with"]["permission-contents"] == "write"
            && token["with"]["permission-pull-requests"] == "write"
            && token["with"]["repositories"] == "${{ github.event.repository.name }}"
            && token["with"]["client-id"] == "${{ secrets.app-id }}"
            && token["with"]["private-key"] == "${{ secrets.app-private-key }}",
        "recording lifecycle must use a repository-scoped App token"
    );
    let verify = super::workflows::step(workflow, "records", "verify_recording")?;
    ensure!(
        verify["env"]["GH_TOKEN"] == "${{ steps.create_app_token.outputs.token }}",
        "recording lifecycle must not create PRs with GITHUB_TOKEN"
    );
    let caller = super::workflows::load(root, ".github/workflows/repository-ci.yaml")?;
    ensure!(
        caller["jobs"]["cargo-crap"]["secrets"]["app-id"]
            == "${{ secrets.CI_APP_CLIENT_ID || secrets.CI_APP_ID }}"
            && caller["jobs"]["cargo-crap"]["secrets"]["app-private-key"]
                == "${{ secrets.CI_APP_PRIVATE_KEY }}",
        "repository CI must forward recording lifecycle App credentials"
    );
    Ok(())
}

fn publication_lock(job: &Value, group: &str) -> Result<()> {
    ensure!(
        job["concurrency"] == json!({"group":group,"cancel-in-progress":false,"queue":"max"}),
        "CRAP publishers require a stable repository-wide queued lock without cancellation"
    );
    Ok(())
}

#[derive(Clone)]
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
    invoke_for_branch(
        root,
        (phase, event_name),
        event,
        responses,
        report,
        "crap/next",
    )
}

fn invoke_for_branch(
    root: &Path,
    (phase, event_name): (&str, &str),
    event: Value,
    responses: Vec<Response>,
    report: Option<&Path>,
    managed: &str,
) -> Result<(bool, String)> {
    let scratch = root.join(".tars/scratch/crap-api");
    fs::create_dir_all(&scratch)?;
    let fixture = tempfile::tempdir_in(scratch)?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let comment_summary =
        report.and_then(|directory| fs::read_to_string(directory.join("summary.md")).ok());
    let expected_title = if managed == "crap/next" {
        "chore: Update CRAP Baseline and Badge"
    } else {
        "test: Verify CRAP Recording PR Lifecycle"
    };
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
            if expected.path == "git/commits" && expected.method == "POST" {
                let body: Value =
                    serde_json::from_str(text.split_once("\r\n\r\n").context("body")?.1)?;
                ensure!(
                    body["message"]
                        == "chore(crap): record baseline and badge\n\nTars-Cloud-CRAP: v1",
                    "recording commits must use a plain Git trailer without Markdown comments"
                );
            }
            if expected.path.starts_with("pulls") && matches!(expected.method, "POST" | "PATCH") {
                let body: Value =
                    serde_json::from_str(text.split_once("\r\n\r\n").context("body")?.1)?;
                ensure!(
                    body.get("title")
                        .is_none_or(|title| title == expected_title),
                    "recording PR creation and refresh must distinguish production and test titles"
                );
            }
            if expected.path.contains("comments") && matches!(expected.method, "POST" | "PATCH") {
                let body: Value =
                    serde_json::from_str(text.split_once("\r\n\r\n").context("body")?.1)?;
                let summary = comment_summary
                    .as_deref()
                    .context("expected comment summary")?;
                ensure!(
                    body["body"]
                        .as_str()
                        .context("comment body")?
                        .contains(summary),
                    "styled summary must survive comment creation and refresh"
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
        .env("INPUT_RECORDS-BRANCH", managed)
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
    merge_groups(root)?;
    pull_request_parents(root)?;
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
    for phase in ["comment", "record"] {
        let (success, _) = invoke(
            root,
            phase,
            "merge_group",
            event.clone(),
            vec![],
            Some(reports.path()),
        )?;
        ensure!(
            !success,
            "merge groups must reject {phase} before API access"
        );
    }
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
        r#"## 🟠 Cargo CRAP: WARNING

> [!WARNING]
> Scores regressed within the threshold. CI passes.
"#,
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
                json!([{"id":10,"user":{"type":"Bot","login":"github-actions[bot]"},"body":format!("{MARKER}\n<!-- analysis:default -->\nold report")} ]),
            ),
            response("PATCH", "issues/comments/10", json!({"id":10})),
        ],
        Some(reports.path()),
    )?;
    ensure!(
        success,
        "sticky comment refresh must preserve the styled report"
    );
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
    recording_lifecycle(root, reports.path())?;
    println!(
        "PASS Cargo CRAP GitHub boundaries: merge parents, trusted artifacts, stale publishers and ownership"
    );
    Ok(())
}

fn pull_request_parents(root: &Path) -> Result<()> {
    let event = json!({"repository":{"default_branch":"trunk"},"pull_request":{"base":{"ref":"trunk","sha":HEAD},"head":{"sha":HEAD}}});
    for (parents, accepted) in [
        (json!([{"sha":BASE},{"sha":HEAD}]), true),
        (json!([{"sha":BASE},{"sha":COMMIT}]), false),
        (json!([{"sha":BASE}]), false),
        (json!([{"sha":BASE},{"sha":HEAD},{"sha":COMMIT}]), false),
        (json!([{"sha":"invalid"},{"sha":HEAD}]), false),
    ] {
        let (success, output) = invoke(
            root,
            "resolve",
            "pull_request",
            event.clone(),
            vec![response(
                "GET",
                &format!("git/commits/{COMMIT}"),
                json!({"parents":parents}),
            )],
            None,
        )?;
        ensure!(success == accepted, "PR merge parent validation: {output}");
        if accepted {
            ensure!(
                output.contains(&format!("baseline-commit={BASE}"))
                    && output.contains(&format!("head-commit={HEAD}"))
                    && output.contains(&format!("commit={COMMIT}")),
                "an older event base must use the captured merge's actual parents: {output}"
            );
        }
    }
    Ok(())
}

fn merge_groups(root: &Path) -> Result<()> {
    let event = json!({"repository":{"default_branch":"trunk"},"merge_group":{"base_ref":"refs/heads/trunk","base_sha":BASE,"head_sha":COMMIT}});
    let path = format!("compare/{BASE}...{COMMIT}?per_page=1");
    let (success, output) = invoke(
        root,
        "resolve",
        "merge_group",
        event.clone(),
        vec![response(
            "GET",
            &path,
            json!({"merge_base_commit":{"sha":BASE}}),
        )],
        None,
    )?;
    ensure!(
        success
            && output.contains(&format!("commit={COMMIT}"))
            && output.contains(&format!("baseline-commit={BASE}"))
            && output.contains("operation=compare")
            && output.contains("head-commit=\n"),
        "merge groups must compare exact event revisions without PR parent checks: {output}"
    );
    for (field, value) in [
        ("base_ref", "refs/heads/other"),
        ("head_sha", HEAD),
        ("head_sha", "invalid"),
        ("base_sha", "invalid"),
    ] {
        let mut invalid = event.clone();
        invalid["merge_group"][field] = json!(value);
        let (success, _) = invoke(root, "resolve", "merge_group", invalid, vec![], None)?;
        ensure!(
            !success,
            "invalid merge group {field} must fail before API access"
        );
    }
    for comparison in [json!({"merge_base_commit":{"sha":HEAD}}), json!({})] {
        let (success, _) = invoke(
            root,
            "resolve",
            "merge_group",
            event.clone(),
            vec![response("GET", &path, comparison)],
            None,
        )?;
        ensure!(!success, "unverified merge group ancestry must fail");
    }
    Ok(())
}

fn recording_lifecycle(root: &Path, reports: &Path) -> Result<()> {
    for name in ["baseline.json", "crap-badge.json"] {
        fs::write(reports.join(name), "{}\n")?;
    }
    let digest = |name: &str| -> Result<String> {
        let output = Command::new("sha256sum").arg(reports.join(name)).output()?;
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
        reports.join("metadata.json"),
        serde_json::to_vec(
            &json!({"complete":true,"operation":"measure","run_id":"42","commit":COMMIT,"baseline_hash":digest("baseline.json")?,"badge_hash":digest("crap-badge.json")?}),
        )?,
    )?;
    let producer = json!({"repository":{"default_branch":"trunk"}});
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
            response(
                "GET",
                "pulls?state=open",
                json!([{"number":8},{"number":9}]),
            ),
        ],
        Some(reports),
    )?;
    ensure!(
        !success && output.contains("More than one managed recording PR"),
        "duplicate recording PRs must fail before writes: {output}"
    );
    let missing = |path: &str| Response {
        method: "GET",
        path: path.into(),
        status: 404,
        body: json!({}),
    };
    for managed in ["crap/next", "tact-crap-records-42-1"] {
        let (success, output) = invoke_for_branch(
            root,
            ("record", "push"),
            producer.clone(),
            vec![
                response(
                    "GET",
                    "git/ref/heads/trunk",
                    json!({"object":{"sha":COMMIT}}),
                ),
                response("GET", "pulls?state=open", json!([])),
                missing(&format!("git/ref/heads/{}", managed.replace('/', "%2F"))),
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
            Some(reports),
            managed,
        )?;
        ensure!(
            success && output.contains("pr-url="),
            "first recording PR creation: {output}"
        );
    }
    let owned = json!({"number":8,"base":{"ref":"trunk"},"head":{"sha":HEAD,"repo":{"full_name":"example/project"}},"user":{"type":"Bot","login":"app[bot]"},"body":MARKER});
    let signed = json!({"commit":{"message":"chore(crap): record baseline and badge\n\nTars-Cloud-CRAP: v1","verification":{"verified":true,"reason":"valid"}},"author":{"type":"Bot","login":"app[bot]"},"committer":{"type":"User","login":"web-flow"}});
    let prefix = |head: Value| {
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
            response("GET", &format!("commits/{HEAD}"), head),
        ]
    };
    for (field, value) in [
        ("/commit/verification/verified", json!(false)),
        ("/commit/verification/reason", json!("unknown_key")),
        ("/author/type", json!("User")),
        ("/author/login", json!("other[bot]")),
        ("/committer/login", json!("another-user")),
        ("/commit/message", json!("chore: manually edit records")),
        (
            "/commit/message",
            json!("chore: copied Tars-Cloud-CRAP: v1"),
        ),
    ] {
        let mut head = signed.clone();
        *head.pointer_mut(field).context("ownership fixture field")? = value;
        let (success, output) = invoke(
            root,
            "record",
            "push",
            producer.clone(),
            prefix(head),
            Some(reports),
        )?;
        ensure!(
            !success && output.contains("unowned head commit"),
            "untrusted commit {field} must fail before writes: {output}"
        );
    }
    let mut refresh = prefix(signed.clone());
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
    let mut contested = refresh.clone();
    contested.pop();
    contested.last_mut().context("guarded update")?.status = 422;
    let (success, output) = invoke(
        root,
        "record",
        "push",
        producer.clone(),
        contested,
        Some(reports),
    )?;
    ensure!(
        !success && output.contains("PATCH git/refs/heads/crap/next: 422"),
        "concurrent branch updates must stop PR publication: {output}"
    );
    let mut legacy_signed = signed.clone();
    legacy_signed["commit"]["message"] = json!(MARKER);
    for head in [signed.clone(), legacy_signed.clone()] {
        let mut responses = refresh.clone();
        responses[4].body = head;
        let (success, output) = invoke(
            root,
            "record",
            "push",
            producer.clone(),
            responses,
            Some(reports),
        )?;
        ensure!(
            success && output.contains("pr-url="),
            "refresh current and legacy managed commits with a guarded branch update: {output}"
        );
    }
    let mut noop = prefix(signed.clone());
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
    let (success, output) = invoke(
        root,
        "record",
        "push",
        producer.clone(),
        noop,
        Some(reports),
    )?;
    ensure!(
        success && output.contains("already match"),
        "identical records close the obsolete PR without creating another: {output}"
    );
    for head in [
        signed,
        legacy_signed,
        json!({"commit":{"message":"Tars-Cloud-CRAP: v1"},"committer":{"type":"Bot","login":"app[bot]"}}),
        json!({"commit":{"message":MARKER},"committer":{"type":"Bot","login":"app[bot]"}}),
    ] {
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
                response(
                    "GET",
                    "git/ref/heads/crap%2Fnext",
                    json!({"object":{"sha":HEAD}}),
                ),
                response("GET", &format!("commits/{HEAD}"), head),
                response(
                    "GET",
                    "contents/.github/crap/baseline.json?",
                    json!({"type":"file","sha":"0967ef424bce6791893e9a57bb952f80fd536e93"}),
                ),
                response(
                    "GET",
                    "contents/.github/badges/crap-badge.json?",
                    json!({"type":"file","sha":"0967ef424bce6791893e9a57bb952f80fd536e93"}),
                ),
            ],
            Some(reports),
        )?;
        ensure!(
            success && output.contains("already match"),
            "owned branch without an open PR must support a no-op: {output}"
        );
    }
    Ok(())
}

fn git(directory: &Path, args: &[&str]) -> Result<String> {
    ensure!(
        directory.is_dir(),
        "No such file or directory: {}",
        directory.display()
    );
    let output = Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
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
    ensure!(
        from.is_dir(),
        "No such file or directory: {}",
        from.display()
    );
    fs::create_dir_all(to)?;
    // Fixture shells may leave profiles and coverage output beside the tracked inputs.
    for name in git(from, &["ls-files", "-z", "--", "."])?
        .split('\0')
        .filter(|name| !name.is_empty())
    {
        if name == "Cargo.lock" {
            continue;
        }
        let target = to.join(name);
        fs::create_dir_all(target.parent().context("fixture file parent")?)?;
        fs::copy(from.join(name), &target)
            .with_context(|| format!("copy tracked fixture file {name}"))?;
    }
    Ok(())
}

fn materialize_workspace(root: &Path, project: &Path) -> Result<()> {
    let workspace =
        fs::read_to_string(root.join("Cargo.toml"))?.parse::<toml_edit::DocumentMut>()?;
    let path = project.join("Cargo.toml");
    let mut manifest = fs::read_to_string(&path)?.parse::<toml_edit::DocumentMut>()?;
    manifest["workspace"] = toml_edit::Item::Table(toml_edit::Table::new());
    manifest["workspace"]["dependencies"] = workspace["workspace"]["dependencies"].clone();
    fs::write(path, manifest.to_string())?;
    fs::copy(root.join("Cargo.lock"), project.join("Cargo.lock"))?;
    let output = Command::new("cargo")
        .args(["update", "--workspace"])
        .current_dir(project)
        .output()?;
    ensure!(
        output.status.success(),
        "resolve standalone fixture from the workspace lockfile: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn native_project(root: &Path, fixture: &Path) -> Result<(PathBuf, String, String)> {
    let project = fixture.join("project");
    fs::create_dir_all(&project)?;
    git(&project, &["init", "-b", "trunk"])?;
    git(&project, &["config", "user.name", "Tact Fixture"])?;
    git(&project, &["config", "user.email", "tact@example.invalid"])?;
    fs::write(project.join("README.md"), "initial\n")?;
    git(&project, &["add", "README.md"])?;
    git(&project, &["commit", "-m", "test: initial empty branch"])?;
    let initial = git(&project, &["rev-parse", "HEAD"])?;
    copy(&root.join("tests/fixtures/cargo-crap"), &project)?;
    materialize_workspace(root, &project)?;
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
    let record = project.join(".github/crap");
    fs::create_dir_all(&record)?;
    fs::write(record.join("baseline.json"), "{}\n")?;
    git(&project, &["add", "."])?;
    git(&project, &["commit", "-m", "test: baseline"])?;
    let base = git(&project, &["rev-parse", "HEAD"])?;
    Ok((project, initial, base))
}

pub(crate) fn prepare_public_fixture(root: &Path) -> Result<()> {
    let scratch = root.join(".tars/scratch/integration");
    fs::create_dir_all(&scratch)?;
    let fixture = tempfile::tempdir_in(scratch)?;
    let (project, _, commit) = native_project(root, fixture.path())?;
    let mut output = fs::OpenOptions::new()
        .append(true)
        .open(std::env::var("GITHUB_OUTPUT").context("fixture step output file")?)?;
    writeln!(output, "directory={}", project.display())?;
    writeln!(output, "commit={commit}")?;
    let _ = fixture.keep();
    Ok(())
}

fn prepare_regression_fixture(project: &Path) -> Result<String> {
    let source = fs::read_to_string(project.join("src/lib.rs"))?;
    fs::write(
        project.join("src/lib.rs"),
        source.replace("assert_eq!(super::choose(false), 2);", ""),
    )?;
    git(project, &["add", "src/lib.rs"])?;
    git(
        project,
        &["commit", "-m", "test: regress coverage within threshold"],
    )?;
    git(project, &["rev-parse", "HEAD"])
}

fn verify_bootstrap_and_debt(
    project: &Path,
    initial: &str,
    base: &str,
    head: &str,
    invoke: impl Fn(&str, &str, &str, &Path) -> Result<PathBuf>,
) -> Result<()> {
    let accepted = invoke("measure", head, base, Path::new(""))?;
    let result: Value = serde_json::from_slice(&fs::read(accepted.join("result.json"))?)?;
    ensure!(
        result["quality"] == "fail" && result["existing_debt"].as_u64().unwrap_or(0) > 0,
        "baseline recording must preserve debt despite absolute gate failure"
    );
    let next = invoke("compare", head, head, &accepted)?;
    let next: Value = serde_json::from_slice(&fs::read(next.join("result.json"))?)?;
    ensure!(
        next["quality"] == "fail"
            && next["severity"] == "error"
            && next["baseline_source"] == "artifact",
        "unchanged accepted debt remains an absolute threshold failure"
    );
    let bootstrap = invoke("compare", head, initial, Path::new(""))?;
    let bootstrap: Value = serde_json::from_slice(&fs::read(bootstrap.join("result.json"))?)?;
    ensure!(
        bootstrap["quality"] == "pass"
            && bootstrap["severity"] == "warning"
            && bootstrap["above_threshold"] == 1
            && bootstrap["baseline_source"] == "bootstrap",
        "unbootstrapped initial branch must warn and pass CI: {bootstrap}"
    );
    ensure!(
        fs::read_to_string(project.join(".cargo-crap.toml"))?.contains("top = 1"),
        "consumer config must remain untouched"
    );
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
    let (project, initial, base) = native_project(root, fixture.path())?;
    let invoke_with_policy = |operation: &str,
                              revision: &str,
                              baseline_revision: &str,
                              baseline_directory: &Path,
                              options: &[(&str, &str)]|
     -> Result<PathBuf> {
        let output = fixture.path().join("output");
        fs::write(&output, "")?;
        let (arch, system) = if std::env::consts::ARCH == "aarch64" {
            ("ARM64", "aarch64-linux")
        } else {
            ("X64", "x86_64-linux")
        };
        let run = Command::new("bash")
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
            .env("ENVIRONMENT_SYSTEM", system)
            .env("FLAKE_SHELL", ".#named")
            .env("CRAP_ACTION_ROOT", root.join("composite/cargo-crap"))
            .env("CRAP_OPERATION", operation)
            .env("CRAP_COMMIT", revision)
            .env("CRAP_BASELINE_COMMIT", baseline_revision)
            .env("CRAP_BASELINE_DIRECTORY", baseline_directory)
            .env("CRAP_COVERAGE_TOOL", backend)
            .env("CRAP_STRICT_BOOTSTRAP", "false")
            .env("CRAP_PACKAGES", "[\"crap-fixture\"]")
            .env("CRAP_FEATURES", "[]")
            .env_remove("CRAP_MANIFEST_DIRECTORY")
            .env_remove("LLVM_COV")
            .env_remove("LLVM_PROFDATA")
            .env_remove("GITHUB_STEP_SUMMARY")
            .envs(options.iter().copied())
            .output()?;
        std::io::stdout().write_all(&run.stdout)?;
        std::io::stderr().write_all(&run.stderr)?;
        let text = fs::read_to_string(&output)?;
        if options
            .iter()
            .any(|(name, _)| *name == "CRAP_FIXTURE_PROFILES")
        {
            ensure!(
                String::from_utf8_lossy(&run.stderr)
                    .lines()
                    .any(|line| line.starts_with("warning:") && line.contains(".profraw:")),
                "unreadable LLVM profiles must leave warnings visible"
            );
        }
        if !run.status.success() {
            ensure!(
                !text.contains("complete=true") && !text.contains("quality="),
                "failed native collection published a completed quality verdict"
            );
        }
        ensure!(
            run.status.success(),
            "native {environment}/{backend} action failed: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        let outputs = crate::output::values(&text)?;
        Ok(PathBuf::from(
            outputs
                .get("report-directory")
                .context("native report directory")?,
        ))
    };
    let invoke =
        |operation: &str, revision: &str, baseline_revision: &str, baseline_directory: &Path| {
            invoke_with_policy(
                operation,
                revision,
                baseline_revision,
                baseline_directory,
                &[],
            )
        };
    let baseline = invoke("measure", &base, &base, Path::new(""))?;
    let warning_head = prepare_regression_fixture(&project)?;
    let warning = invoke("compare", &warning_head, &base, &baseline)?;
    let warning: Value = serde_json::from_slice(&fs::read(warning.join("result.json"))?)?;
    ensure!(
        warning["quality"] == "pass"
            && warning["severity"] == "warning"
            && warning["above_threshold"] == 0,
        "native regression warning: {warning}"
    );
    let source = fs::read_to_string(project.join("src/lib.rs"))?;
    fs::write(
        project.join("src/lib.rs"),
        source
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
            && result["severity"] == "error"
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
    verify_bootstrap_and_debt(&project, &initial, &base, &head, invoke)?;
    let strict = invoke_with_policy(
        "compare",
        &head,
        &initial,
        Path::new(""),
        &[("CRAP_STRICT_BOOTSTRAP", "true")],
    )?;
    let result: Value = serde_json::from_slice(&fs::read(strict.join("result.json"))?)?;
    ensure!(
        result["quality"] == "fail"
            && result["severity"] == "error"
            && result["above_threshold"] == 1
            && result["baseline_source"] == "bootstrap",
        "strict bootstrap must fail above the threshold: {result}"
    );
    let summary = fs::read_to_string(strict.join("summary.md"))?;
    ensure!(
        summary.contains("CI fails") && !summary.contains("CI passes"),
        "strict bootstrap summary must report failure"
    );
    if backend == "llvm-cov" {
        verify_llvm_profile_collection(&project.join("profile-merge.sh"), |options| {
            invoke_with_policy("measure", &head, &base, Path::new(""), options)
        })?;
    }
    println!(
        "PASS Cargo CRAP native {environment}/{backend}: coverage, warnings, threshold errors, artifact reuse, fresh fallback, recorded debt, bootstrap, strict bootstrap"
    );
    Ok(())
}

fn verify_llvm_profile_collection(
    wrapper: &Path,
    invoke: impl Fn(&[(&str, &str)]) -> Result<PathBuf>,
) -> Result<()> {
    let wrapper = wrapper.to_str().context("LLVM fixture wrapper path")?;
    for (case, mode, succeeds) in [
        ("mixed", "any", false),
        ("mixed", "all", true),
        ("all-invalid", "all", false),
    ] {
        let result = invoke(&[
            ("CRAP_LLVM_PROFILE_MERGE_FAILURE_MODE", mode),
            ("CRAP_FIXTURE_LLVM_PROFDATA", wrapper),
            ("CRAP_FIXTURE_PROFILES", case),
        ]);
        if succeeds {
            let reports = result?;
            let metadata: Value =
                serde_json::from_slice(&fs::read(reports.join("metadata.json"))?)?;
            ensure!(
                metadata["complete"] == true
                    && metadata["profile"]["llvm_profile_merge"]["failure_mode"] == mode,
                "mixed LLVM collection must produce validated scores with policy provenance"
            );
            let verdict: Value = serde_json::from_slice(&fs::read(reports.join("result.json"))?)?;
            ensure!(
                verdict["quality"] == "fail" && verdict["above_threshold"] == 1,
                "merging partial profiles must preserve the absolute threshold"
            );
        } else {
            let error = result
                .err()
                .context("invalid LLVM collection unexpectedly completed")?
                .to_string();
            ensure!(
                error.contains("warning:") && error.contains("no score gate evaluated"),
                "invalid profiles must warn and fail analysis: {error}"
            );
        }
    }
    for mode in ["any", "all"] {
        let result = invoke(&[
            ("CRAP_LLVM_PROFILE_MERGE_FAILURE_MODE", mode),
            ("CRAP_FIXTURE_FAIL_TEST", "true"),
        ]);
        let error = result
            .err()
            .context("failing test unexpectedly completed analysis")?
            .to_string();
        ensure!(
            error.contains("no score gate evaluated"),
            "test failures must stop analysis: {error}"
        );
    }
    println!(
        "PASS native LLVM merge policies: mixed profiles, all-invalid collection, failed tests and warnings"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn copied_consumer_inherits_dependencies_and_builds_with_its_own_lockfile() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture = tempfile::tempdir().unwrap();
        let project = fixture.path();
        std::fs::create_dir(project.join("src")).unwrap();
        for name in ["Cargo.toml", "src/lib.rs"] {
            std::fs::copy(
                root.join("tests/fixtures/cargo-crap").join(name),
                project.join(name),
            )
            .unwrap();
        }
        let original = std::fs::read_to_string(root.join("Cargo.lock")).unwrap();
        super::materialize_workspace(&root, project).unwrap();
        let manifest = std::fs::read_to_string(project.join("Cargo.toml"))
            .unwrap()
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        let workspace = std::fs::read_to_string(root.join("Cargo.toml"))
            .unwrap()
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        assert_eq!(
            manifest["workspace"]["dependencies"].to_string(),
            workspace["workspace"]["dependencies"].to_string()
        );
        assert_eq!(
            manifest["dependencies"]["cargo-crap"]["optional"].as_bool(),
            Some(true)
        );
        let lock = std::fs::read_to_string(project.join("Cargo.lock")).unwrap();
        assert!(lock.contains("name = \"cargo-crap\""));
        assert!(!lock.contains("name = \"tact\""));
        assert_eq!(
            std::fs::read_to_string(root.join("Cargo.lock")).unwrap(),
            original
        );
        let output = std::process::Command::new("cargo")
            .args(["check", "--locked", "--offline"])
            .current_dir(project)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn publication_jobs_serialize_across_runs_and_callers() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for (path, job, group) in [
            (
                ".github/workflows/consumer-cargo-crap.yaml",
                "record",
                "cargo-crap-records-${{ github.repository }}",
            ),
            (
                ".github/workflows/test-cargo-crap.yaml",
                "records",
                "cargo-crap-lifecycle-${{ github.repository }}",
            ),
        ] {
            let workflow = crate::checks::workflows::load(&root, path).unwrap();
            let mut job = workflow["jobs"][job].clone();
            super::publication_lock(&job, group).unwrap();
            for invalid in [
                serde_json::Value::Null,
                serde_json::json!({"group":format!("{group}-${{{{ github.run_id }}}}"),"cancel-in-progress":false,"queue":"max"}),
                serde_json::json!({"group":format!("{group}-${{{{ github.workflow }}}}"),"cancel-in-progress":false,"queue":"max"}),
                serde_json::json!({"group":group,"cancel-in-progress":true,"queue":"max"}),
                serde_json::json!({"group":group,"cancel-in-progress":false}),
            ] {
                job["concurrency"] = invalid;
                assert!(super::publication_lock(&job, group).is_err());
            }
        }
    }

    #[test]
    fn recording_lifecycle_rejects_the_default_token() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut workflow =
            crate::checks::workflows::load(&root, ".github/workflows/test-cargo-crap.yaml")
                .unwrap();
        super::recording_contract(&root, &workflow).unwrap();
        let steps = workflow["jobs"]["records"]["steps"].as_array_mut().unwrap();
        let verify = steps
            .iter_mut()
            .find(|step| step["id"] == "verify_recording")
            .unwrap();
        verify["env"]["GH_TOKEN"] = serde_json::json!("${{ github.token }}");
        assert!(
            super::recording_contract(&root, &workflow)
                .unwrap_err()
                .to_string()
                .contains("must not create PRs with GITHUB_TOKEN")
        );
    }

    #[test]
    fn quality_step_warns_without_failing_and_errors_fail() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let workflow =
            crate::checks::workflows::load(&root, ".github/workflows/consumer-cargo-crap.yaml")
                .unwrap();
        let gate = crate::checks::workflows::step(&workflow, "analyze", "enforce_quality").unwrap();
        for (quality, severity, succeeds, annotation) in [
            ("pass", "info", true, "::notice::"),
            ("pass", "warning", true, "::warning::"),
            ("fail", "error", false, "::error::"),
            ("pass", "unknown", false, "::error::"),
        ] {
            let output = std::process::Command::new(crate::runner::executable("bash").unwrap())
                .args(["-e", "-c", gate["run"].as_str().unwrap()])
                .env_clear()
                .env("QUALITY", quality)
                .env("SEVERITY", severity)
                .output()
                .unwrap();
            assert_eq!(output.status.success(), succeeds);
            assert!(String::from_utf8_lossy(&output.stdout).contains(annotation));
        }
    }

    #[test]
    fn native_lifecycle_checks_warning_and_absolute_error_levels() {
        use std::{fs, path::Path};

        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path();
        let consumer = root.join("tests/fixtures/cargo-crap");
        fs::create_dir_all(consumer.join("src")).unwrap();
        fs::write(root.join("Cargo.toml"), "[workspace.dependencies]\n").unwrap();
        fs::write(root.join("Cargo.lock"), "version = 4\n").unwrap();
        fs::write(
            consumer.join("Cargo.toml"),
            "[package]\nname = 'crap-fixture'\nversion = '0.1.0'\n",
        )
        .unwrap();
        fs::write(consumer.join("devenv.nix"), "{ source = ../../..; }\n").unwrap();
        fs::write(
            consumer.join("src/lib.rs"),
            "assert_eq!(super::choose(false), 2);\n",
        )
        .unwrap();
        super::git(root, &["init", "-b", "trunk"]).unwrap();
        super::git(root, &["add", "tests"]).unwrap();
        let scripts = root.join("composite/cargo-crap/scripts");
        fs::create_dir_all(&scripts).unwrap();
        fs::write(scripts.join("dispatch.sh"), r#"set -euo pipefail
[[ $PROJECT_DIRECTORY == . && $CRAP_PACKAGES == '["crap-fixture"]' ]]
[[ $CRAP_FEATURES == '[]' && $RUNNER_OS == Linux ]]
[[ $GITHUB_WORKSPACE == "$RUNNER_TEMP/project" ]]
report=$(mktemp -d "$RUNNER_TEMP/report-XXXXXXXX")
count=0
[[ ! -f $RUNNER_TEMP/count ]] || read -r count < "$RUNNER_TEMP/count"
((count+=1))
printf '%s\n' "$count" > "$RUNNER_TEMP/count"
case $count in
    1) [[ $CRAP_OPERATION == measure ]]; result='{"quality":"pass","severity":"info","existing_debt":0}' ;;
    2) [[ $CRAP_OPERATION == compare && -n $CRAP_BASELINE_DIRECTORY ]]; result='{"quality":"pass","severity":"warning","regressed":1,"above_threshold":0}' ;;
    3) [[ $CRAP_OPERATION == compare && -n $CRAP_BASELINE_DIRECTORY ]]; result='{"quality":"fail","severity":"error","regressed":1,"new_above":1,"baseline_source":"artifact"}' ;;
    4) [[ $CRAP_OPERATION == compare && -z $CRAP_BASELINE_DIRECTORY ]]; result='{"quality":"fail","severity":"error","baseline_source":"fresh"}' ;;
    5) [[ $CRAP_OPERATION == measure ]]; result='{"quality":"fail","severity":"error","existing_debt":1}' ;;
    6) [[ $CRAP_COMMIT == "$CRAP_BASELINE_COMMIT" ]]; result='{"quality":"fail","severity":"error","baseline_source":"artifact"}' ;;
    7) [[ $CRAP_OPERATION == compare && -z $CRAP_BASELINE_DIRECTORY && $CRAP_COMMIT != "$CRAP_BASELINE_COMMIT" ]]; result='{"quality":"pass","severity":"warning","above_threshold":1,"baseline_source":"bootstrap"}' ;;
    8) [[ $CRAP_OPERATION == compare && $CRAP_STRICT_BOOTSTRAP == true ]]; result='{"quality":"fail","severity":"error","above_threshold":1,"baseline_source":"bootstrap"}'; printf 'CI fails\n' > "$report/summary.md" ;;
    9|11) printf 'warning: invalid.profraw: truncated profile data\ncoverage failed; no score gate evaluated\n' >&2; exit 7 ;;
    10) [[ $CRAP_LLVM_PROFILE_MERGE_FAILURE_MODE == all ]]; printf 'warning: invalid.profraw: truncated profile data\n' >&2; result='{"quality":"fail","above_threshold":1}'; printf '{"complete":true,"profile":{"llvm_profile_merge":{"failure_mode":"all"}}}\n' > "$report/metadata.json" ;;
    12|13) [[ $CRAP_FIXTURE_FAIL_TEST == true ]]; printf 'coverage failed; no score gate evaluated\n' >&2; exit 7 ;;
    *) exit 1 ;;
esac
printf '%s\n' "$result" > "$report/result.json"
printf 'report-directory=%s\n' "$report" >> "$GITHUB_OUTPUT"
"#).unwrap();
        for (backend, environment) in [("llvm-cov", "devenv"), ("tarpaulin", "flakes")] {
            super::native(root, backend, environment).unwrap();
        }
        assert!(super::native(root, "unknown", "devenv").is_err());
        assert!(super::native(root, "llvm-cov", "unknown").is_err());
        assert!(Path::new(&scripts.join("dispatch.sh")).is_file());
    }

    #[test]
    fn native_fixture_copy_ignores_generated_environment_state() {
        use std::os::unix::fs::symlink;

        let scratch = tempfile::tempdir().unwrap();
        let source = scratch.path().join("source");
        let target = scratch.path().join("target");
        std::fs::create_dir_all(source.join("src")).unwrap();
        std::fs::write(source.join("Cargo.toml"), "fixture manifest\n").unwrap();
        std::fs::write(source.join("src/lib.rs"), "pub fn fixture() {}\n").unwrap();
        super::git(&source, &["init", "-b", "trunk"]).unwrap();
        super::git(&source, &["add", "Cargo.toml", "src/lib.rs"]).unwrap();
        std::fs::create_dir_all(source.join(".devenv")).unwrap();
        symlink(source.join("src"), source.join(".devenv/profile")).unwrap();
        std::fs::write(source.join("lcov.info"), "generated coverage\n").unwrap();

        super::copy(&source, &target).unwrap();

        assert_eq!(
            std::fs::read_to_string(target.join("src/lib.rs")).unwrap(),
            "pub fn fixture() {}\n"
        );
        assert!(target.join("Cargo.toml").is_file());
        assert!(!target.join(".devenv").exists());
        assert!(!target.join("lcov.info").exists());
        assert!(!target.join(".git").exists());
    }

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
