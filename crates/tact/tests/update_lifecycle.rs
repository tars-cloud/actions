use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output},
};

struct Fixture {
    directory: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let scratch = repository.join(".tars/scratch/update-lifecycle");
        fs::create_dir_all(&scratch).unwrap();
        let directory = tempfile::tempdir_in(scratch).unwrap();
        let root = directory.path();
        fs::create_dir(root.join("bin")).unwrap();
        fs::create_dir_all(root.join("tests/fixtures/devenv-update")).unwrap();
        for name in ["devenv.yaml", "devenv.lock"] {
            fs::copy(
                repository.join("tests/fixtures/devenv-update").join(name),
                root.join("tests/fixtures/devenv-update").join(name),
            )
            .unwrap();
        }
        fs::write(root.join("output"), "").unwrap();
        let bash = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|p| p.join("bash"))
            .find(|p| p.is_file())
            .unwrap();
        let script = r#"
set -euo pipefail
method=$3; path=$4
body=null
if [[ $# -gt 4 ]]; then body=$(cat); fi
jq -cn --arg method "$method" --arg path "$path" --argjson body "$body" '{method:$method,path:$path,body:$body}' >> "$FIXTURE_ROOT/calls"
[[ ${FAIL_API:-false} != true ]] || { printf 'fixture denied' >&2; exit 7; }
case "$method:$path" in
    GET:*/git/commits/*) printf '{"tree":{"sha":"source-tree"}}' ;;
    POST:*/git/trees) printf '{"sha":"new-tree"}' ;;
    POST:*/git/commits) printf '{"sha":"new-commit"}' ;;
    GET:*/git/ref/heads/*) printf '{"object":{"sha":"parent"}}' ;;
    GET:*/pulls\?state=all\&base=tact-update-base-12-1\&head=example:tact-update-pr-12-1)
        if [[ ${PR_STATE:-open} == missing ]]; then printf '[]'; else
          printf '[{"state":"%s","number":7,"head":{"sha":"pr-head"}}]' "${PR_STATE:-open}"; fi ;;
    GET:*/pulls/7/files) printf '[{"filename":"%s"}]' "${PR_FILE:-devenv.lock}" ;;
    GET:*/commits/pr-head) printf '{"commit":{"verification":{"verified":%s},"tree":{"sha":"pr-tree"}}}' "${SIGNED:-true}" ;;
    GET:*/actions/runs\?*) printf '{"workflow_runs":[{"path":".github/workflows/test-devenv-update-probe.yaml","conclusion":"success"}]}' ;;
    GET:*/git/matching-refs/heads/*)
        name=${path##*/}
        if [[ ${REFS_EXIST:-true} == true ]]; then printf '[{"ref":"refs/heads/%s"}]' "$name"; else printf '[]'; fi ;;
    POST:*/git/refs|PATCH:*/git/refs/heads/*|PATCH:*/pulls/7|DELETE:*/git/refs/heads/*) ;;
    *) printf 'Unexpected request: %s' "$*" >&2; exit 1 ;;
esac
"#;
        let gh = root.join("bin/gh");
        fs::write(&gh, format!("#!{}\n{script}", bash.display())).unwrap();
        fs::set_permissions(gh, fs::Permissions::from_mode(0o700)).unwrap();
        Self { directory }
    }

    fn command(&self, phase: &str) -> Command {
        let root = self.directory.path();
        let mut command = Command::new(env!("CARGO_BIN_EXE_tact"));
        command
            .args([
                "--root",
                root.to_str().unwrap(),
                "ci",
                "update-lifecycle",
                phase,
            ])
            .env_clear()
            .env(
                "PATH",
                std::env::join_paths(
                    std::iter::once(root.join("bin"))
                        .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
                )
                .unwrap(),
            )
            .env("FIXTURE_ROOT", root)
            .env("GITHUB_REPOSITORY", "example/actions")
            .env("GITHUB_RUN_ID", "12")
            .env("GITHUB_RUN_ATTEMPT", "1")
            .env("GITHUB_SHA", "source")
            .env("GITHUB_OUTPUT", root.join("output"));
        if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
            command.env("LLVM_PROFILE_FILE", profile);
        }
        command
    }

    fn calls(&self) -> Vec<Value> {
        fs::read_to_string(self.directory.path().join("calls"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

fn success(output: Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn lifecycle_prepares_advances_verifies_reconciles_and_cleans_its_own_branches() {
    let fixture = Fixture::new();
    for phase in ["prepare", "advance", "verify", "reconcile"] {
        success(fixture.command(phase).output().unwrap());
    }
    success(
        fixture
            .command("closed")
            .env("PR_STATE", "closed")
            .output()
            .unwrap(),
    );
    success(fixture.command("cleanup").output().unwrap());
    let calls = fixture.calls();
    let paths: Vec<_> = calls.iter().map(|c| c["path"].as_str().unwrap()).collect();
    assert_eq!(
        paths,
        [
            "repos/example/actions/git/commits/source",
            "repos/example/actions/git/trees",
            "repos/example/actions/git/commits",
            "repos/example/actions/git/refs",
            "repos/example/actions/git/ref/heads/tact-update-base-12-1",
            "repos/example/actions/git/commits/parent",
            "repos/example/actions/git/trees",
            "repos/example/actions/git/commits",
            "repos/example/actions/git/refs/heads/tact-update-base-12-1",
            "repos/example/actions/pulls?state=all&base=tact-update-base-12-1&head=example:tact-update-pr-12-1",
            "repos/example/actions/pulls/7/files",
            "repos/example/actions/commits/pr-head",
            "repos/example/actions/actions/runs?head_sha=pr-head&event=pull_request",
            "repos/example/actions/pulls?state=all&base=tact-update-base-12-1&head=example:tact-update-pr-12-1",
            "repos/example/actions/pulls/7/files",
            "repos/example/actions/commits/pr-head",
            "repos/example/actions/git/ref/heads/tact-update-base-12-1",
            "repos/example/actions/git/commits",
            "repos/example/actions/git/refs/heads/tact-update-base-12-1",
            "repos/example/actions/pulls?state=all&base=tact-update-base-12-1&head=example:tact-update-pr-12-1",
            "repos/example/actions/pulls?state=all&base=tact-update-base-12-1&head=example:tact-update-pr-12-1",
            "repos/example/actions/pulls/7",
            "repos/example/actions/git/matching-refs/heads/tact-update-pr-12-1",
            "repos/example/actions/git/refs/heads/tact-update-pr-12-1",
            "repos/example/actions/git/matching-refs/heads/tact-update-base-12-1",
            "repos/example/actions/git/refs/heads/tact-update-base-12-1"
        ]
    );
    let files = calls[1]["body"]["tree"].as_array().unwrap();
    let lock: Value = serde_json::from_str(
        files.iter().find(|f| f["path"] == "devenv.lock").unwrap()["content"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(lock["nodes"]["nixpkgs"]["locked"]["lastModified"], 1);
    assert_eq!(calls[2]["body"]["parents"], json!(["source"]));
    assert_eq!(
        calls[17]["body"],
        json!({"message":"test: Resolve Update on the Fixture Base", "tree":"pr-tree", "parents":["parent"]})
    );
    assert_eq!(
        fs::read_to_string(fixture.directory.path().join("output")).unwrap(),
        "base=tact-update-base-12-1\nbranch=tact-update-pr-12-1\npr-number=7\n"
    );
}

#[test]
fn rejects_missing_closed_unsigned_or_unexpected_file_prs_before_reconciliation() {
    for (name, value, message) in [
        ("PR_STATE", "missing", "expected one managed PR"),
        ("PR_STATE", "closed", "expected open PR"),
        ("SIGNED", "false", "must be signed"),
        ("PR_FILE", "Cargo.toml", "unexpected PR files"),
    ] {
        let fixture = Fixture::new();
        let output = fixture
            .command("reconcile")
            .env(name, value)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains(message));
        assert!(fixture.calls().iter().all(|c| c["method"] == "GET"));
    }
}

#[test]
fn cleanup_is_idempotent_and_api_failures_keep_diagnostics() {
    let fixture = Fixture::new();
    success(
        fixture
            .command("cleanup")
            .env("PR_STATE", "closed")
            .env("REFS_EXIST", "false")
            .output()
            .unwrap(),
    );
    assert!(fixture.calls().iter().all(|c| c["method"] == "GET"));
    let output = fixture
        .command("advance")
        .env("FAIL_API", "true")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("fixture denied"));
    assert!(
        !fixture
            .command("prepare")
            .env("GITHUB_RUN_ID", "../escape")
            .output()
            .unwrap()
            .status
            .success()
    );
}
