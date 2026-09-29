use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[test]
fn documentation_only_stale_candidate_is_repaired_and_keeps_refreshing() {
    let fixture = Fixture::new();
    let sha = fixture.change("fix: correct cache behavior");
    fixture.after_ci(&sha);
    fixture.change("docs: make the release candidate stale");
    fixture.merge_release();
    let output = fixture.invoke(&["prepare"], None);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("PR created"),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(fixture.read(OPEN)[0][0]["title"], "chore(release): v0.1.1");
    let later = fixture.change("docs: advance trunk again before merging repair");
    assert!(fixture.after_ci(&later).contains("PR refreshed"));
    let head = fixture.release_head();
    assert_eq!(fixture.git(&["rev-parse", &format!("{head}^")]), later);
    assert_eq!(fixture.read(OPEN)[0].as_array().unwrap().len(), 1);
}

#[test]
fn retry_repairs_pr_metadata_after_a_branch_was_already_updated() {
    let fixture = Fixture::new();
    let sha = fixture.change("feat: add shared functionality");
    fixture.after_ci(&sha);
    let head = fixture.release_head();
    let mut response = fixture.read(OPEN);
    response[0][0]["title"] = json!("chore(release): v0.1.1");
    response[0][0]["body"] = json!("stale metadata after interrupted preparation");
    fixture.reply(OPEN, response);
    assert!(fixture.after_ci(&sha).contains("PR refreshed"));
    assert_eq!(fixture.release_head(), head);
    assert_eq!(fixture.read(OPEN)[0][0]["title"], "chore(release): v0.2.0");
}

#[test]
fn publication_does_not_wait_for_newer_trunk_ci() {
    let fixture = Fixture::new();
    let sha = fixture.change("fix: correct cache behavior");
    fixture.after_ci(&sha);
    let release = fixture.merge_release();
    let later = fixture.change("feat: add the next feature");
    fixture.ci(&later, "pending");
    let output = fixture.after_ci(&release);
    assert!(output.contains("Published"));
    assert!(output.contains("Awaiting successful trunk CI"));
    assert!(fixture.read(OPEN)[0].as_array().unwrap().is_empty());
    fixture.ci(&later, "success");
    assert!(fixture.after_ci(&later).contains("PR created"));
}

#[test]
fn manual_preparation_can_repair_a_stale_merged_candidate() {
    let fixture = Fixture::new();
    let sha = fixture.change("fix: correct cache behavior");
    fixture.after_ci(&sha);
    fixture.change("feat: merge while the release PR was stale");
    let release = fixture.merge_release();
    let output = fixture.invoke(&["after-ci"], Some(&release));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("stale release PR"));
    let output = fixture.invoke(&["prepare"], None);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Rebuilding an invalid"));
    assert_eq!(fixture.read(OPEN)[0][0]["title"], "chore(release): v0.2.0");
}

#[test]
fn first_release_can_create_a_pr_without_changing_the_initial_cargo_version() {
    let fixture = Fixture::new();
    fixture.git(&["tag", "--delete", "v0.1.0", "v0"]);
    fixture.git(&["push", "origin", ":refs/tags/v0.1.0", ":refs/tags/v0"]);
    let sha = fixture.change("chore: initialize the release process");
    assert!(fixture.after_ci(&sha).contains("PR created"));
    assert_eq!(fixture.read(OPEN)[0][0]["title"], "chore(release): v0.1.0");
    let head = fixture.release_head();
    assert_eq!(
        fixture.git(&["diff", "--name-only", &sha, &head]),
        "CHANGELOG.md"
    );
}

#[test]
fn fork_release_branches_are_not_modified() {
    let fixture = Fixture::new();
    let sha = fixture.change("fix: correct cache behavior");
    fixture.reply(
        OPEN,
        json!([[{"number":1, "head":{"repo":{"full_name":"owner/fork"}}}]]),
    );
    let output = fixture.invoke(&["after-ci"], Some(&sha));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("must belong to this repository"));
    assert!(fixture.writes().is_empty());
}

const OPEN: &str = "pulls?state=open&base=trunk&head=owner:release/next&per_page=100";
const CLOSED: &str = "pulls?state=closed&base=trunk&head=owner:release/next&per_page=100";

fn run(root: &Path, program: &str, args: &[&str]) -> String {
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{program} {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

struct Fixture {
    directory: tempfile::TempDir,
    root: PathBuf,
    remote: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("work");
        let remote = directory.path().join("remote.git");
        fs::create_dir_all(&root).unwrap();
        let fixture = Self {
            directory,
            root,
            remote,
        };
        fixture.git(&["init", "--initial-branch=trunk"]);
        fixture.git(&["config", "user.name", "Release fixture"]);
        fixture.git(&["config", "user.email", "test@example.invalid"]);
        fs::write(
            fixture.root.join(".convco"),
            include_str!("../../../.convco"),
        )
        .unwrap();
        fs::write(fixture.root.join("Cargo.toml"), "[workspace]\nmembers = [\"tact\", \"actions-release\"]\nresolver = \"3\"\n[workspace.package]\nversion = \"0.1.0\"\n").unwrap();
        for name in ["tact", "actions-release"] {
            fs::create_dir_all(fixture.root.join(name).join("src")).unwrap();
            fs::write(
                fixture.root.join(name).join("Cargo.toml"),
                format!(
                    "[package]\nname = \"{name}\"\nversion.workspace = true\nedition = \"2024\"\n"
                ),
            )
            .unwrap();
            fs::write(
                fixture.root.join(name).join("src/main.rs"),
                "fn main() {}\n",
            )
            .unwrap();
        }
        run(&fixture.root, "cargo", &["generate-lockfile", "--offline"]);
        fixture.commit("chore: initialize release fixture");
        fixture.git(&["tag", "v0.1.0"]);
        fixture.git(&["tag", "v0"]);
        fixture.git(&["init", "--bare", fixture.remote.to_str().unwrap()]);
        fixture.git(&["remote", "add", "origin", fixture.remote.to_str().unwrap()]);
        fixture.git(&["push", "origin", "trunk", "--tags"]);
        fs::create_dir_all(fixture.directory.path().join("bin")).unwrap();
        fs::create_dir_all(fixture.directory.path().join("responses")).unwrap();
        let mock = fixture.directory.path().join("bin/gh");
        let bash = run(&fixture.root, "bash", &["-c", "command -v bash"]);
        fs::write(
            &mock,
            include_str!("fixtures/gh.bash").replacen(
                "#!/usr/bin/env bash",
                &format!("#!{bash}"),
                1,
            ),
        )
        .unwrap();
        fs::set_permissions(mock, fs::Permissions::from_mode(0o755)).unwrap();
        fixture.reply(OPEN, json!([[]]));
        fixture.reply(CLOSED, json!([[]]));
        fixture.reply("releases?per_page=100", json!([[]]));
        fs::write(fixture.directory.path().join("writes"), "").unwrap();
        fixture
    }

    fn git(&self, args: &[&str]) -> String {
        run(&self.root, "git", args)
    }

    fn commit(&self, message: &str) -> String {
        self.git(&["add", "."]);
        self.git(&["commit", "--allow-empty", "-m", message]);
        self.git(&["rev-parse", "HEAD"])
    }

    fn trunk(&self) {
        self.git(&["reset", "--hard"]);
        self.git(&["checkout", "trunk"]);
    }

    fn change(&self, message: &str) -> String {
        self.trunk();
        let sha = self.commit(message);
        self.git(&["push", "origin", "trunk"]);
        self.ci(&sha, "success");
        self.reply(&format!("commits/{sha}/pulls?per_page=100"), json!([]));
        sha
    }

    fn response_path(&self, path: &str) -> PathBuf {
        self.directory
            .path()
            .join("responses")
            .join(format!("repos_owner_actions_{path}").replace('/', "_"))
    }

    fn reply(&self, path: &str, value: Value) {
        fs::write(
            self.response_path(path),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
    }

    fn read(&self, path: &str) -> Value {
        serde_json::from_slice(&fs::read(self.response_path(path)).unwrap()).unwrap()
    }

    fn ci(&self, sha: &str, conclusion: &str) {
        self.reply(&format!("actions/workflows/repository-ci.yaml/runs?head_sha={sha}&event=push&branch=trunk&per_page=100"), json!({"workflow_runs": [{
            "head_sha":sha, "head_branch":"trunk", "event":"push", "head_repository":{"full_name":"owner/actions"},
            "status":if conclusion == "pending" { "in_progress" } else { "completed" }, "conclusion":conclusion
        }]}));
    }

    fn invoke(&self, args: &[&str], trigger: Option<&str>) -> Output {
        self.trunk();
        let event = self.directory.path().join("event.json");
        fs::write(&event, serde_json::to_vec(&json!({"action":"completed", "workflow_run":{
            "name":"Repository: CI", "event":"push", "head_branch":"trunk", "head_repository":{"full_name":"owner/actions"},
            "status":"completed", "conclusion":"success", "head_sha":trigger
        }})).unwrap()).unwrap();
        let paths = std::iter::once(self.directory.path().join("bin"))
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap()))
            .collect::<Vec<_>>();
        Command::new(env!("CARGO_BIN_EXE_actions-release"))
            .args(args)
            .current_dir(&self.root)
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GITHUB_ACTIONS", "true")
            .env("GITHUB_REF", "refs/heads/trunk")
            .env(
                "GITHUB_EVENT_NAME",
                if trigger.is_some() {
                    "workflow_run"
                } else {
                    "workflow_dispatch"
                },
            )
            .env("GITHUB_REPOSITORY", "owner/actions")
            .env("GITHUB_EVENT_PATH", &event)
            .env("GH_TOKEN", "fixture-write-token")
            .env("GH_READ_TOKEN", "fixture-read-token")
            .env("GITHUB_STEP_SUMMARY", self.directory.path().join("summary"))
            .env("RELEASE_FIXTURE", self.directory.path())
            .env("RELEASE_REMOTE", &self.remote)
            .output()
            .unwrap()
    }

    fn after_ci(&self, sha: &str) -> String {
        let output = self.invoke(&["after-ci"], Some(sha));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn writes(&self) -> String {
        fs::read_to_string(self.directory.path().join("writes")).unwrap()
    }

    fn release_head(&self) -> String {
        self.git(&["ls-remote", "origin", "refs/heads/release/next"])
            .split_whitespace()
            .next()
            .unwrap()
            .to_owned()
    }

    fn merge_release(&self) -> String {
        let head = self.release_head();
        let mut pr = self.read(OPEN)[0][0].clone();
        self.trunk();
        self.git(&["merge", "--squash", &head]);
        let commit = self.commit(pr["title"].as_str().unwrap());
        self.git(&["push", "origin", "trunk"]);
        self.git(&["push", "origin", &format!("{head}:refs/pull/42/head")]);
        pr["merged_at"] = json!("2026-01-01T00:00:00Z");
        pr["merge_commit_sha"] = json!(commit);
        self.reply(OPEN, json!([[]]));
        self.reply(CLOSED, json!([[pr.clone()]]));
        self.reply(&format!("commits/{commit}/pulls?per_page=100"), json!([pr]));
        self.ci(&commit, "success");
        commit
    }
}

#[test]
fn one_pr_refreshes_across_merges_and_unchanged_retries_do_not_push() {
    let fixture = Fixture::new();
    let fix = fixture.change("fix: correct cache behavior");
    assert!(fixture.after_ci(&fix).contains("PR created"));
    let first = fixture.release_head();
    assert!(fixture.after_ci(&fix).contains("already current"));
    assert_eq!(fixture.release_head(), first);
    let feature = fixture.change("feat: add a shared workflow");
    assert!(fixture.after_ci(&feature).contains("PR refreshed"));
    assert_eq!(fixture.read(OPEN)[0][0]["title"], "chore(release): v0.2.0");
    let docs = fixture.change("docs: describe the workflow");
    fixture.after_ci(&docs);
    let head = fixture.release_head();
    assert_eq!(fixture.git(&["rev-parse", &format!("{head}^")]), docs);
    assert_eq!(
        fixture
            .writes()
            .matches("POST repos/owner/actions/pulls\n")
            .count(),
        1
    );
    assert_eq!(fixture.read(OPEN)[0].as_array().unwrap().len(), 1);
    assert!(!fixture.writes().contains("git/refs"));
}

#[test]
fn chores_are_successful_noops_but_dependabot_builds_prepare_a_patch() {
    let fixture = Fixture::new();
    let chore = fixture.change("chore: maintain repository metadata");
    assert!(fixture.after_ci(&chore).contains("Nothing to release"));
    assert!(fixture.writes().is_empty());
    let dependency = fixture.change("build(deps): update a pinned dependency");
    fixture.after_ci(&dependency);
    assert_eq!(fixture.read(OPEN)[0][0]["title"], "chore(release): v0.1.1");
}

#[test]
fn out_of_order_ci_waits_for_latest_trunk_then_prepares_that_revision() {
    let fixture = Fixture::new();
    let first = fixture.change("fix: correct first behavior");
    let latest = fixture.change("feat: add newer behavior");
    fixture.ci(&latest, "pending");
    assert!(
        fixture
            .after_ci(&first)
            .contains("Awaiting successful trunk CI")
    );
    assert!(fixture.writes().is_empty());
    fixture.ci(&latest, "success");
    fixture.after_ci(&first);
    let head = fixture.release_head();
    assert_eq!(fixture.git(&["rev-parse", &format!("{head}^")]), latest);
}

#[test]
fn duplicate_release_prs_fail_before_any_remote_changes() {
    let fixture = Fixture::new();
    let sha = fixture.change("fix: correct cache behavior");
    fixture.reply(OPEN, json!([[{"number":1}, {"number":2}]]));
    let output = fixture.invoke(&["after-ci"], Some(&sha));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("multiple release PRs"));
    assert!(fixture.writes().is_empty());
}

#[test]
fn merged_release_waits_for_publication_then_prepares_accumulated_changes() {
    let fixture = Fixture::new();
    let sha = fixture.change("fix: correct cache behavior");
    fixture.after_ci(&sha);
    let release = fixture.merge_release();
    let later = fixture.change("feat: add the next feature");
    let before = fixture.writes();
    assert!(fixture.after_ci(&later).contains("Awaiting publication"));
    let manual = fixture.invoke(&["prepare"], None);
    assert!(manual.status.success());
    assert!(String::from_utf8_lossy(&manual.stdout).contains("Awaiting publication"));
    assert_eq!(fixture.writes(), before);
    let output = fixture.after_ci(&release);
    assert!(output.contains("Published https://github.com/owner/actions/releases/tag/v0.1.1"));
    assert!(output.contains("PR created"));
    assert_eq!(
        fixture
            .git(&["ls-remote", "origin", "refs/tags/v0.1.1"])
            .split_whitespace()
            .next(),
        Some(release.as_str())
    );
    assert_eq!(fixture.read(OPEN)[0][0]["title"], "chore(release): v0.2.0");
    let before_retry = fixture.writes();
    assert!(fixture.after_ci(&release).contains("already published"));
    assert_eq!(fixture.writes(), before_retry);
}

#[test]
fn release_merge_publishes_once_without_preparing_another_release() {
    let fixture = Fixture::new();
    let sha = fixture.change("fix: correct cache behavior");
    fixture.after_ci(&sha);
    let release = fixture.merge_release();
    let output = fixture.after_ci(&release);
    assert!(output.contains("Published"));
    assert!(output.contains("Nothing to release"));
    assert!(fixture.read(OPEN)[0].as_array().unwrap().is_empty());
    let before = fixture.writes();
    fixture.after_ci(&release);
    assert_eq!(fixture.writes(), before);
}

#[test]
fn partial_publication_blocks_preparation_and_manual_retry_finishes_it() {
    let fixture = Fixture::new();
    let sha = fixture.change("fix: correct cache behavior");
    fixture.after_ci(&sha);
    let release = fixture.merge_release();
    fixture.git(&["tag", "v0.1.1", &release]);
    fixture.git(&["push", "origin", "v0.1.1"]);
    fixture.reply(
        "releases?per_page=100",
        json!([[{"tag_name":"v0.1.1", "draft":false, "prerelease":false}]]),
    );
    let later = fixture.change("docs: document the release");
    assert!(fixture.after_ci(&later).contains("Awaiting publication"));
    let output = fixture.invoke(&["publish", "--commit", &release], None);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fixture
            .git(&["ls-remote", "origin", "refs/tags/v0"])
            .split_whitespace()
            .next(),
        Some(release.as_str())
    );
    assert!(
        !fixture
            .writes()
            .contains("POST repos/owner/actions/releases\n")
    );
}
