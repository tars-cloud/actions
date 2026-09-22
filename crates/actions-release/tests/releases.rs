use std::fs;
use std::path::Path;
use std::process::{Command, Output};

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

fn git(root: &Path, args: &[&str]) -> String {
    run(root, "git", args)
}

fn commit(root: &Path, message: &str) -> String {
    git(root, &["add", "."]);
    git(root, &["commit", "--allow-empty", "-m", message]);
    git(root, &["rev-parse", "HEAD"])
}

fn repo() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    git(root, &["init", "--initial-branch=trunk"]);
    git(root, &["config", "user.email", "test@example.invalid"]);
    git(root, &["config", "user.name", "Release tests"]);
    fs::write(root.join(".convco"), include_str!("../../../.convco")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[workspace.package]\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(root.join("Cargo.lock"), lock("0.1.0")).unwrap();
    commit(root, "chore: initialize repository");
    directory
}

fn lock(version: &str) -> String {
    format!(
        "version = 4\n[[package]]\nname = \"tact\"\nversion = \"{version}\"\n[[package]]\nname = \"actions-release\"\nversion = \"{version}\"\n"
    )
}

#[test]
fn convco_policy_matches_repository_semver() {
    for (message, expected) in [
        ("feat(setup-foo): add a new action", "0.2.0"),
        ("fix(cache): correct cache discovery", "0.1.1"),
        ("build(deps): update a dependency", "0.1.1"),
        ("feat(nix)!: require a newer runner", "1.0.0"),
        ("docs: explain setup inputs", "0.1.0"),
    ] {
        let directory = repo();
        let root = directory.path();
        git(root, &["tag", "v0.1.0"]);
        commit(root, message);
        assert_eq!(
            run(root, "convco", &["version", "--bump"]),
            expected,
            "{message}"
        );
    }
}

#[test]
fn first_release_and_dependency_batch_use_convco_output() {
    let directory = repo();
    let root = directory.path();
    assert_eq!(run(root, "convco", &["version", "--bump"]), "0.1.0");
    git(root, &["tag", "v0.1.0"]);
    git(root, &["tag", "v0"]);
    for message in [
        "build(deps): update first dependency",
        "build(deps): update second dependency",
    ] {
        commit(root, message);
    }
    assert_eq!(run(root, "convco", &["version", "--bump"]), "0.1.1");
    let changelog = run(root, "convco", &["changelog", "--unreleased", "0.1.1"]);
    assert!(
        changelog.contains("Dependencies")
            && changelog.contains("first dependency")
            && changelog.contains("second dependency")
    );
    assert!(changelog.contains("github.com/tars-cloud/actions"));
    commit(root, "feat: add another composite action");
    assert_eq!(run(root, "convco", &["version", "--bump"]), "0.2.0");
}

fn candidate(root: &Path) -> (String, String) {
    git(root, &["tag", "v0.1.0"]);
    commit(root, "fix(cache): correct cache discovery");
    let base = git(root, &["rev-parse", "HEAD"]);
    git(root, &["checkout", "-b", "release/next"]);
    fs::write(
        root.join("Cargo.toml"),
        "[workspace.package]\nversion = \"0.1.1\"\n",
    )
    .unwrap();
    fs::write(root.join("Cargo.lock"), lock("0.1.1")).unwrap();
    let changelog = run(root, "convco", &["changelog", "--unreleased", "0.1.1"]);
    fs::write(root.join("CHANGELOG.md"), changelog).unwrap();
    let head = commit(root, "chore(release): v0.1.1");
    (base, head)
}

fn merge(root: &Path) -> String {
    git(root, &["checkout", "trunk"]);
    git(root, &["merge", "--squash", "release/next"]);
    commit(root, "chore(release): v0.1.1 (#2)")
}

fn verify(root: &Path, merged: &str, head: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_actions-release"))
        .args(["verify-candidate", "--commit", merged, "--head", head])
        .current_dir(root)
        .output()
        .unwrap()
}

#[test]
fn verifies_squash_release_and_retry_after_tag_creation() {
    let directory = repo();
    let root = directory.path();
    let (_, head) = candidate(root);
    let merged = merge(root);
    let output = verify(root, &merged, &head);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    git(root, &["tag", "v0.1.1", &merged]);
    assert!(verify(root, &merged, &head).status.success());
    commit(root, "docs: explain next feature");
    assert!(
        verify(root, &merged, &head).status.success(),
        "later trunk commits must not enter the release"
    );
}

#[test]
fn rejects_release_merged_after_trunk_advanced() {
    let directory = repo();
    let root = directory.path();
    let (_, head) = candidate(root);
    git(root, &["checkout", "trunk"]);
    commit(root, "feat: add another action");
    let merged = merge(root);
    let output = verify(root, &merged, &head);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("stale release PR"));
}

#[test]
fn rejects_incorrect_version_lockfile_and_non_release_changes() {
    for file in ["Cargo.toml", "Cargo.lock", "unexpected.txt"] {
        let directory = repo();
        let root = directory.path();
        candidate(root);
        let content = match file {
            "Cargo.toml" => "[workspace.package]\nversion = \"9.0.0\"\n".to_owned(),
            "Cargo.lock" => lock("0.1.0"),
            _ => "not a release file".to_owned(),
        };
        fs::write(root.join(file), content).unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "--amend", "--no-edit"]);
        let head = git(root, &["rev-parse", "HEAD"]);
        let merged = merge(root);
        assert!(
            !verify(root, &merged, &head).status.success(),
            "accepted invalid {file}"
        );
    }
}
