use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn consumer_release_requires_an_exact_commit_before_contacting_github() {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_actions-release"))
        .args(["rust", "candidate", "--commit", "main"])
        .current_dir(directory.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("40-character commit SHA"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

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
    branch: String,
}

impl Fixture {
    fn new(branch: &str, workspace: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("work");
        let remote = directory.path().join("remote.git");
        fs::create_dir_all(&root).unwrap();
        let fixture = Self {
            directory,
            root,
            remote,
            branch: branch.to_owned(),
        };
        fixture.git(&["init", &format!("--initial-branch={branch}")]);
        fixture.git(&["config", "user.name", "Release fixture"]);
        fixture.git(&["config", "user.email", "test@example.invalid"]);
        if workspace {
            fs::write(fixture.root.join("Cargo.toml"), "[workspace]\nmembers = [\"cli\", \"core\"]\nresolver = \"3\"\n[workspace.package]\nversion = \"1.2.3\"\n[workspace.dependencies]\ncore = { path = \"core\", version = \"1.2.3\" }\n").unwrap();
            for name in ["cli", "core"] {
                fs::create_dir_all(fixture.root.join(name).join("src")).unwrap();
                fs::write(fixture.root.join(name).join("Cargo.toml"), format!("[package]\nname = \"{name}\"\nversion.workspace = true\nedition = \"2024\"\n{}", if name == "cli" { "[dependencies]\ncore.workspace = true\n" } else { "" })).unwrap();
                fs::write(
                    fixture.root.join(name).join("src/lib.rs"),
                    "pub fn example() {}\n",
                )
                .unwrap();
            }
        } else {
            fs::create_dir_all(fixture.root.join("src")).unwrap();
            fs::write(
                fixture.root.join("Cargo.toml"),
                "[package]\nname = \"consumer\"\nversion = \"1.2.3\"\nedition = \"2024\"\n",
            )
            .unwrap();
            fs::write(fixture.root.join("src/lib.rs"), "pub fn example() {}\n").unwrap();
        }
        run(&fixture.root, "cargo", &["generate-lockfile", "--offline"]);
        fixture.commit("chore: initialize consumer");
        fixture.git(&["tag", "v1.2.3"]);
        fixture.git(&["init", "--bare", fixture.remote.to_str().unwrap()]);
        fixture.git(&["remote", "add", "origin", fixture.remote.to_str().unwrap()]);
        fixture.git(&["push", "origin", branch, "--tags"]);
        fs::create_dir_all(fixture.directory.path().join("bin")).unwrap();
        let mock = fixture.directory.path().join("bin/gh");
        let bash = run(&fixture.root, "bash", &["-c", "command -v bash"]);
        fs::write(
            &mock,
            include_str!("fixtures/rust-gh.bash").replacen(
                "#!/usr/bin/env bash",
                &format!("#!{bash}"),
                1,
            ),
        )
        .unwrap();
        fs::set_permissions(mock, fs::Permissions::from_mode(0o755)).unwrap();
        fixture.reply("repository.json", json!({"default_branch":branch}));
        for file in ["open.json", "closed.json", "releases.json", "assets.json"] {
            fixture.reply(file, json!([]));
        }
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
    fn change(&self, message: &str) -> String {
        self.git(&["checkout", &self.branch]);
        let sha = self.commit(message);
        self.git(&["push", "origin", &self.branch]);
        sha
    }
    fn reply(&self, path: &str, value: Value) {
        fs::write(
            self.directory.path().join(path),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
    }
    fn read(&self, path: &str) -> Value {
        serde_json::from_slice(&fs::read(self.directory.path().join(path)).unwrap()).unwrap()
    }
    fn invoke(&self, command: &str, commit: &str) -> std::process::Output {
        self.invocation(command, commit, false).output().unwrap()
    }
    fn invocation(&self, command: &str, commit: &str, bootstrap: bool) -> Command {
        self.git(&["checkout", "--detach", commit]);
        let mut paths = std::iter::once(self.directory.path().join("bin"))
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap()))
            .collect::<Vec<_>>();
        let boundary = self.directory.path().join("runner-path-boundary");
        if bootstrap {
            paths.push(boundary.clone());
            paths.push(self.directory.path().join("ambient-runner-tools"));
        }
        let action_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../composite/release-rust")
            .canonicalize()
            .unwrap();
        let mut cli = if bootstrap {
            let mut cli = Command::new("bash");
            cli.arg(action_root.join("scripts/release.sh"));
            cli.env("RELEASE_ACTION_ROOT", action_root)
                .env("RELEASE_COMMAND", command)
                .env("RELEASE_COMMIT", commit)
                .env("RELEASE_PATH_BOUNDARY", boundary)
                .env("RUNNER_TEMP", self.directory.path());
            cli
        } else {
            let mut cli = Command::new(env!("CARGO_BIN_EXE_actions-release"));
            cli.args(["rust", command, "--commit", commit]);
            cli
        };
        if command == "publish" && !bootstrap {
            cli.args([
                "--artifact-run-id",
                "77",
                "--manifest-artifact",
                "release-manifest",
            ]);
        }
        cli.current_dir(&self.root)
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GITHUB_ACTIONS", "true")
            .env("GITHUB_EVENT_NAME", "push")
            .env("GITHUB_SHA", commit)
            .env("GITHUB_RUN_ID", "77")
            .env("GITHUB_REF", format!("refs/heads/{}", self.branch))
            .env("GITHUB_REPOSITORY", "owner/consumer")
            .env("GH_TOKEN", "fixture-write-token")
            .env("GH_READ_TOKEN", "fixture-read-token")
            .env("RELEASE_FIXTURE", self.directory.path())
            .env("RELEASE_REMOTE", &self.remote)
            .env(
                "DEVENV_RESULT_FILE",
                self.directory.path().join("result.json"),
            )
            .env("GITHUB_STEP_SUMMARY", self.directory.path().join("summary"));
        cli
    }
    fn success(&self, command: &str, commit: &str) -> Value {
        let output = self.invoke(command, commit);
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        self.read("result.json")
    }
    fn merge(&self) -> String {
        let mut pr = self.read("open.json")[0].clone();
        let head = pr["head"]["sha"].as_str().unwrap().to_owned();
        self.git(&["checkout", &self.branch]);
        self.git(&["merge", "--squash", &head]);
        let commit = self.commit(pr["title"].as_str().unwrap());
        self.git(&["push", "origin", &self.branch]);
        self.git(&[
            "--git-dir",
            self.remote.to_str().unwrap(),
            "update-ref",
            "refs/pull/42/head",
            &head,
        ]);
        pr["merged_at"] = json!("2026-09-27T12:00:00Z");
        pr["merge_commit_sha"] = json!(commit);
        self.reply("closed.json", json!([pr]));
        self.reply(&format!("commit-{commit}.json"), self.read("closed.json"));
        self.reply("open.json", json!([]));
        commit
    }
    fn artifacts(&self, commit: &str) {
        let root = self.directory.path().join("artifacts");
        fs::create_dir_all(root.join("release-manifest")).unwrap();
        fs::create_dir_all(root.join("binary")).unwrap();
        fs::write(root.join("binary/consumer"), "hello\n").unwrap();
        fs::write(
            root.join("binary/consumer.sha256"),
            "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03  consumer\n",
        )
        .unwrap();
        let manifest = json!({"schema":1,"version":"1.2.4","tag":"v1.2.4","commit_sha":commit,
            "assets":[{"artifact":"binary","files":["consumer","consumer.sha256"]}],"reference_artifacts":[]});
        fs::write(
            root.join("release-manifest/release-manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        self.reply("artifacts.json", json!({"artifacts":[{"name":"release-manifest","expired":false},{"name":"binary","expired":false}]}));
        self.reply("run.json", json!({"head_sha":commit,"head_branch":self.branch,"head_repository":{"full_name":"owner/consumer"},"event":"push","status":"in_progress"}));
    }
}

#[test]
fn ordinary_merges_refresh_one_pr_and_publish_only_after_the_human_merge() {
    let fixture = Fixture::new("main", false);
    let first = fixture.change("fix: correct the consumer");
    assert_eq!(
        fixture.success("candidate", &first)["prepare-ready"],
        "true"
    );
    assert_eq!(fixture.success("prepare", &first)["version"], "1.2.4");
    let later = fixture.change("fix: handle another case");
    assert_eq!(fixture.success("prepare", &later)["version"], "1.2.4");
    assert_eq!(fixture.read("open.json").as_array().unwrap().len(), 1);
    let head = fixture.read("open.json")[0]["head"]["sha"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(fixture.git(&["rev-parse", &format!("{head}^")]), later);
    let merged = fixture.merge();
    let candidate = fixture.success("candidate", &merged);
    assert_eq!(candidate["prepare-ready"], "false");
    assert_eq!(candidate["release-ready"], "true");
    fixture.artifacts(&merged);
    assert_eq!(fixture.success("publish", &merged)["tag"], "v1.2.4");
    assert_eq!(fixture.git(&["rev-parse", "v1.2.4"]), merged);
    assert_eq!(fixture.read("releases.json")[0]["draft"], false);
    assert_eq!(fixture.read("assets.json").as_array().unwrap().len(), 2);
    assert_eq!(
        fixture.success("candidate", &merged)["release-ready"],
        "false"
    );
    let before = fs::read(fixture.directory.path().join("writes")).unwrap();
    fixture.success("publish", &merged);
    assert_eq!(
        fs::read(fixture.directory.path().join("writes")).unwrap(),
        before
    );
}

#[test]
fn workspace_bump_updates_internal_requirements_and_lockfile() {
    let fixture = Fixture::new("master", true);
    let commit = fixture.change("feat: add a new API");
    assert_eq!(fixture.success("prepare", &commit)["version"], "1.3.0");
    let manifest = fs::read_to_string(fixture.root.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("version = \"1.3.0\" }"));
    let lock = fs::read_to_string(fixture.root.join("Cargo.lock")).unwrap();
    assert_eq!(lock.matches("version = \"1.3.0\"").count(), 2);
    let merged = fixture.merge();
    assert_eq!(
        fixture.success("candidate", &merged)["release-ready"],
        "true"
    );
}

#[test]
fn chores_do_not_open_a_release_pr() {
    let fixture = Fixture::new("trunk", false);
    let commit = fixture.change("chore(deps): update tools");
    assert!(
        fixture.success("prepare", &commit)["reason"]
            .as_str()
            .unwrap()
            .contains("No version bump")
    );
    assert!(fixture.read("open.json").as_array().unwrap().is_empty());
}

#[test]
fn interrupted_publication_keeps_a_draft_and_blocks_another_release() {
    let fixture = Fixture::new("main", false);
    let commit = fixture.change("fix: correct the consumer");
    fixture.success("prepare", &commit);
    let merged = fixture.merge();
    fixture.artifacts(&merged);
    fs::write(fixture.directory.path().join("fail-upload"), "").unwrap();
    assert!(!fixture.invoke("publish", &merged).status.success());
    assert_eq!(fixture.read("releases.json")[0]["draft"], true);
    assert_eq!(fixture.read("assets.json")[0]["state"], "starter");
    let later = fixture.change("feat: start the next release");
    assert_eq!(
        fixture.success("candidate", &later)["prepare-ready"],
        "false"
    );
    fs::remove_file(fixture.directory.path().join("fail-upload")).unwrap();
    fixture.success("publish", &merged);
    assert_eq!(
        fixture.success("candidate", &later)["prepare-ready"],
        "true"
    );
}

impl Fixture {
    fn ready() -> (Self, String) {
        let fixture = Self::new("main", false);
        let commit = fixture.change("fix: correct the consumer");
        fixture.success("prepare", &commit);
        let merged = fixture.merge();
        fixture.artifacts(&merged);
        (fixture, merged)
    }

    fn manifest(&self) -> Value {
        self.read("artifacts/release-manifest/release-manifest.json")
    }

    fn reject_publication(&self, commit: &str, expected: &str) {
        let before = fs::read(self.directory.path().join("writes")).unwrap();
        let result = self.invoke("publish", commit);
        assert!(
            !result.status.success(),
            "unexpected success for {expected}"
        );
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(expected),
            "expected {expected}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            fs::read(self.directory.path().join("writes")).unwrap(),
            before,
            "invalid publication caused a mutation"
        );
    }
}

#[test]
fn incomplete_or_untrusted_artifacts_cannot_create_a_draft() {
    for scenario in [
        "missing",
        "expired",
        "fork",
        "commit",
        "pr",
        "manifest",
        "duplicate",
        "traversal",
        "symlink",
        "checksum",
        "unknown",
    ] {
        let (fixture, commit) = Fixture::ready();
        let mut manifest = fixture.manifest();
        let expected = match scenario {
            "missing" => {
                fixture.reply(
                    "artifacts.json",
                    json!({"artifacts":[{"name":"release-manifest","expired":false}]}),
                );
                "missing or expired"
            }
            "expired" => {
                let mut artifacts = fixture.read("artifacts.json");
                artifacts["artifacts"][1]["expired"] = json!(true);
                fixture.reply("artifacts.json", artifacts);
                "missing or expired"
            }
            "fork" | "commit" | "pr" => {
                let mut run = fixture.read("run.json");
                match scenario {
                    "fork" => run["head_repository"]["full_name"] = json!("fork/consumer"),
                    "commit" => run["head_sha"] = json!("a".repeat(40)),
                    _ => run["event"] = json!("pull_request"),
                }
                fixture.reply("run.json", run);
                "artifacts must come from"
            }
            "manifest" => {
                manifest["version"] = json!("99.0.0");
                "manifest identity differs"
            }
            "duplicate" => {
                manifest["assets"][0]["files"] = json!(["consumer", "consumer"]);
                "duplicate release asset"
            }
            "traversal" => {
                manifest["assets"][0]["files"] = json!(["../consumer"]);
                "without traversal"
            }
            "symlink" => {
                let file = fixture.directory.path().join("artifacts/binary/consumer");
                fs::remove_file(&file).unwrap();
                std::os::unix::fs::symlink(fixture.root.join("Cargo.toml"), file).unwrap();
                "symlinks are not allowed"
            }
            "checksum" => {
                fs::write(
                    fixture.directory.path().join("artifacts/binary/consumer"),
                    "corrupt\n",
                )
                .unwrap();
                "checksum mismatch"
            }
            _ => {
                manifest["run"] = json!("unexpected shell hook");
                "unknown field"
            }
        };
        fixture.reply("artifacts/release-manifest/release-manifest.json", manifest);
        fixture.reject_publication(&commit, expected);
    }
}

#[test]
fn explicit_empty_assets_and_image_references_publish_without_build_assumptions() {
    let (fixture, commit) = Fixture::ready();
    let mut manifest = fixture.manifest();
    manifest["assets"] = json!([]);
    manifest["reference_artifacts"] = json!([{"artifact":"images","file":"references.json"}]);
    fixture.reply("artifacts/release-manifest/release-manifest.json", manifest);
    fs::create_dir_all(fixture.directory.path().join("artifacts/images")).unwrap();
    fixture.reply("artifacts/images/references.json", json!({"schema":1,"version":"1.2.4","commit_sha":commit,
        "references":[{"name":"Container Image", "url":"https://example.invalid/packages/app", "reference":"registry.example.invalid/app:v1.2.4", "digest":format!("sha256:{}", "b".repeat(64))}]}));
    let mut artifacts = fixture.read("artifacts.json");
    artifacts["artifacts"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"images","expired":false}));
    fixture.reply("artifacts.json", artifacts);
    fixture.success("publish", &commit);
    assert!(fixture.read("assets.json").as_array().unwrap().is_empty());
    let release = fixture.read("releases.json");
    assert!(
        release[0]["body"]
            .as_str()
            .unwrap()
            .contains("registry.example.invalid/app:v1.2.4")
    );
    assert!(
        release[0]["body"]
            .as_str()
            .unwrap()
            .contains("correct the consumer")
    );
}

#[test]
fn existing_version_tags_and_drafts_cannot_redirect_publication() {
    let (fixture, commit) = Fixture::ready();
    fixture.git(&["tag", "v1.2.4", "v1.2.3"]);
    fixture.git(&["push", "origin", "refs/tags/v1.2.4"]);
    fixture.reject_publication(&commit, "immutable version tag");
    let (fixture, commit) = Fixture::ready();
    fixture.reply("releases.json", json!([{"id":9,"tag_name":"v1.2.4", "draft":true, "prerelease":false, "target_commitish":"a".repeat(40)}]));
    fixture.reject_publication(&commit, "existing draft targets another commit");
}

#[test]
fn first_release_uses_root_cargo_version_and_api_failure_is_not_absence() {
    let fixture = Fixture::new("main", false);
    fixture.git(&["tag", "--delete", "v1.2.3"]);
    fixture.git(&["push", "origin", ":refs/tags/v1.2.3"]);
    let commit = fixture.change("chore: initialize releases");
    assert_eq!(fixture.success("prepare", &commit)["version"], "1.2.3");
    let merged = fixture.merge();
    assert_eq!(
        fixture.success("candidate", &merged)["release-ready"],
        "true"
    );
    fs::write(fixture.directory.path().join("fail-api"), "").unwrap();
    fixture.reject_publication(&merged, "fixture transport failure");
}

#[test]
fn stale_release_pr_is_rejected_after_a_later_merge() {
    let fixture = Fixture::new("main", false);
    let commit = fixture.change("fix: correct the consumer");
    fixture.success("prepare", &commit);
    fixture.change("docs: advance the base before release merge");
    let merged = fixture.merge();
    let output = fixture.invoke("candidate", &merged);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("stale release PR"));
}

#[test]
fn version_updates_preserve_unrelated_workspace_metadata() {
    let fixture = Fixture::new("main", true);
    let path = fixture.root.join("Cargo.toml");
    let manifest = fs::read_to_string(&path).unwrap();
    fs::write(path, format!("{manifest}\n[workspace.metadata.example.dependencies]\ncore = {{ path = \"core\", version = \"1.2.3\" }}\n")).unwrap();
    let commit = fixture.change("feat: add workspace metadata");
    fixture.success("prepare", &commit);
    let edited: toml_edit::DocumentMut = fs::read_to_string(fixture.root.join("Cargo.toml"))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        edited["workspace"]["metadata"]["example"]["dependencies"]["core"]["version"].as_str(),
        Some("1.2.3")
    );
}

#[test]
fn bundled_action_bootstrap_runs_from_a_separate_consumer_checkout() {
    let fixture = Fixture::new("main", false);
    fs::create_dir_all(fixture.root.join(".cargo")).unwrap();
    fs::write(
        fixture.root.join(".cargo/config.toml"),
        "[build]\ntarget = \"consumer-only-target\"\n",
    )
    .unwrap();
    let commit = fixture.change("fix: bootstrap example");
    let result = fixture
        .invocation("candidate", &commit, true)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fixture.read("result.json")["prepare-ready"], "true");
    assert!(!fixture.root.join("target").exists());
    assert!(
        fs::read(fixture.directory.path().join("writes"))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn independent_member_versions_and_unmanaged_prs_fail_clearly() {
    let fixture = Fixture::new("main", true);
    let manifest = fixture.root.join("cli/Cargo.toml");
    fs::write(
        &manifest,
        fs::read_to_string(&manifest)
            .unwrap()
            .replace("version.workspace = true", "version = \"1.2.3\""),
    )
    .unwrap();
    let commit = fixture.change("fix: example");
    let result = fixture.invoke("prepare", &commit);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("must inherit version.workspace"));

    let (fixture, commit) = Fixture::ready();
    let mut prs = fixture.read(&format!("commit-{commit}.json"));
    prs[0]["body"] = json!("Not a managed PR");
    fixture.reply(&format!("commit-{commit}.json"), prs);
    fixture.reject_publication(&commit, "not created by this shared release process");
}

#[test]
fn published_release_assets_are_immutable() {
    let (fixture, commit) = Fixture::ready();
    fixture.success("publish", &commit);
    let mut assets = fixture.read("assets.json");
    assets[0]["digest"] = json!(format!("sha256:{}", "0".repeat(64)));
    fixture.reply("assets.json", assets);
    fixture.reject_publication(&commit, "published releases are never modified");
}
