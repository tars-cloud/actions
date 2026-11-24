use serde_json::json;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    process::{Command, Output},
};

struct Fixture {
    directory: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::create_dir(root.join("bin")).unwrap();
        fs::write(
            root.join("event.json"),
            json!({"repository":{"default_branch":"trunk"}}).to_string(),
        )
        .unwrap();
        let gh = r#"#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$FIXTURE_ROOT/calls"
method=$3; path=$4
if [[ $# -gt 4 ]]; then
    body=$(cat)
    printf '%s\n' "$body" >> "$FIXTURE_ROOT/bodies"
fi
case "$method:$path" in
    GET:*/git/commits/commit1) printf '{"tree":{"sha":"tree1"},"parents":[]}' ;;
    GET:*/git/commits/commit2) printf '{"tree":{"sha":"tree2"},"parents":[{"sha":"commit1"}]}' ;;
    GET:*/git/commits/commit3) printf '{"tree":{"sha":"tree3"},"parents":[{"sha":"commit2"}]}' ;;
    GET:*/git/commits/*) printf '{"tree":{"sha":"tree"}}' ;;
    POST:*/git/commits) printf '{"sha":"commit"}' ;;
    GET:*/git/trees/tree1\?recursive=1) printf '{"truncated":false,"tree":[{"path":"baseline.json","type":"blob"},{"path":"crap-badge.json","type":"blob"}]}' ;;
    GET:*/git/trees/tree3\?recursive=1) printf '{"truncated":false,"tree":[{"path":"baseline.json","type":"blob"},{"path":"crap-badge.json","type":"blob"},{"path":"extra/baseline.json","type":"blob"},{"path":"extra/crap-badge.json","type":"blob"}]}' ;;
    POST:*/git/refs|PATCH:*/git/refs/heads/*|DELETE:*/git/refs/heads/*) ;;
    *) printf 'Unexpected API request: %s' "$*" >&2; exit 1 ;;
esac
"#;
        let node = r#"#!/usr/bin/env bash
set -euo pipefail
[[ $INPUT_PHASE == record && $INPUT_TOKEN == fixture-token ]]
[[ $GITHUB_EVENT_NAME == push && $GITHUB_REF == refs/heads/tact-update-base-crap-12-1 ]]
count=0
[[ ! -f $FIXTURE_ROOT/count ]] || read -r count < "$FIXTURE_ROOT/count"
((count+=1))
printf '%s\n' "$count" > "$FIXTURE_ROOT/count"
env | sort > "$FIXTURE_ROOT/publisher-env"
directory=$(printenv 'INPUT_REPORT-DIRECTORY')
jq -e '.complete == true and .operation == "measure" and .run_id == "12"' "$directory/metadata.json" >/dev/null
[[ $(sha256sum "$directory/baseline.json" | cut -d' ' -f1) == $(jq -r .baseline_hash "$directory/metadata.json") ]]
[[ $(sha256sum "$directory/crap-badge.json" | cut -d' ' -f1) == $(jq -r .badge_hash "$directory/metadata.json") ]]
[[ ${FAIL_PUBLISH:-} != "$count" ]] || exit 7
printf 'records-branch=%s\n' "$(printenv 'INPUT_RECORDS-BRANCH')" >> "$GITHUB_OUTPUT"
case "$count" in
    1) printf 'records-commit=commit1\n' >> "$GITHUB_OUTPUT" ;;
    2) printf 'records-commit=commit2\n' >> "$GITHUB_OUTPUT" ;;
    3) [[ $(printenv 'INPUT_ANALYSIS-ID') == extra ]]; printf 'records-commit=commit3\n' >> "$GITHUB_OUTPUT" ;;
    4) [[ $(printenv 'INPUT_ANALYSIS-ID') == extra ]]; printf 'records-commit=\n' >> "$GITHUB_OUTPUT" ;;
    *) exit 1 ;;
esac
"#;
        let bash = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|directory| directory.join("bash"))
            .find(|path| path.is_file())
            .expect("selected Bash for fixture scripts");
        for (name, script) in [("gh", gh), ("node", node)] {
            let path = root.join("bin").join(name);
            // Nix build sandboxes have no /usr/bin/env interpreter.
            let body = script.strip_prefix("#!/usr/bin/env bash\n").unwrap();
            fs::write(&path, format!("#!{}\n{body}", bash.display())).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self { directory }
    }

    fn command(&self) -> Command {
        let root = self.directory.path();
        let mut command = Command::new(env!("CARGO_BIN_EXE_tact"));
        command
            .args(["--root", root.to_str().unwrap(), "ci", "crap-lifecycle"])
            .env_clear()
            .env(
                "PATH",
                std::env::join_paths(
                    std::iter::once(root.join("bin"))
                        .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
                )
                .unwrap(),
            )
            .env("CI", "true")
            .env("FIXTURE_ROOT", root)
            .env("GH_TOKEN", "fixture-token")
            .env("GITHUB_REPOSITORY", "example/actions")
            .env("GITHUB_RUN_ID", "12")
            .env("GITHUB_RUN_ATTEMPT", "1")
            .env("GITHUB_EVENT_PATH", root.join("event.json"))
            .env("GITHUB_EVENT_NAME", "push")
            .env("GITHUB_REF", "refs/heads/trunk")
            .env("GITHUB_SHA", "a".repeat(40));
        if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
            command.env("LLVM_PROFILE_FILE", profile);
        }
        command
    }
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn fixture_exercises_orphan_branch_create_append_multi_analysis_noop_and_cleanup() {
    let fixture = Fixture::new();
    success(&fixture.command().output().unwrap());
    let calls = fs::read_to_string(fixture.directory.path().join("calls")).unwrap();
    assert_eq!(
        calls
            .lines()
            .filter(|line| line.contains("--method DELETE"))
            .count(),
        2
    );
    assert!(calls.contains("git/trees/tree1?recursive=1"));
    assert!(calls.contains("git/trees/tree3?recursive=1"));
    let bodies = fs::read_to_string(fixture.directory.path().join("bodies")).unwrap();
    assert_eq!(
        bodies
            .lines()
            .filter(|line| line.contains("\"force\":false"))
            .count(),
        1
    );
}

#[test]
fn publisher_failure_still_deletes_temporary_branches() {
    let fixture = Fixture::new();
    let output = fixture.command().env("FAIL_PUBLISH", "2").output().unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("real managed publisher failed"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let calls = fs::read_to_string(fixture.directory.path().join("calls")).unwrap();
    assert_eq!(
        calls
            .lines()
            .filter(|line| line.contains("--method DELETE"))
            .count(),
        2
    );
}

#[test]
fn lifecycle_rejects_untrusted_contexts_before_any_api_call() {
    let fixture = Fixture::new();
    for (name, value) in [
        ("CI", "false"),
        ("GITHUB_EVENT_NAME", "pull_request"),
        ("GITHUB_REF", "refs/heads/topic"),
        ("GITHUB_RUN_ID", "../escape"),
    ] {
        assert!(
            !fixture
                .command()
                .env(name, value)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    assert!(!fixture.directory.path().join("calls").exists());
}
