use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use tempfile::TempDir;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn fixture(case: Value) -> TempDir {
    let scratch = repository().join(".tars/scratch/tact-cli");
    fs::create_dir_all(&scratch).unwrap();
    let root = tempfile::tempdir_in(scratch).unwrap();
    fs::create_dir_all(root.path().join("composite/sample")).unwrap();
    fs::write(
        root.path().join("composite/sample/action.yml"),
        "---\nname: sample\n",
    )
    .unwrap();
    write_manifest(root.path(), json!({"version": 1, "tests": [case]}));
    root
}

fn write_manifest(root: &Path, manifest: Value) {
    fs::write(
        root.join("composite/sample/test.yaml"),
        format!(
            "---\n{}\n",
            serde_json::to_string_pretty(&manifest).unwrap()
        ),
    )
    .unwrap();
}

fn case() -> Value {
    json!({
        "id": "example", "description": "Example scenario",
        "command": ["bash", "-c", "exit 0"],
        "expect": {"exit": 0, "calls": []}
    })
}

fn cli(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tact"))
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

fn success(output: &Output) -> String {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn failure(output: &Output, expected: &str) {
    assert!(!output.status.success(), "unexpected success: {output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(expected),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn help_and_argument_errors() {
    let root = fixture(case());
    assert!(success(&cli(root.path(), &["--help"])).contains("Test actions"));
    assert_eq!(cli(root.path(), &["unknown"]).status.code(), Some(2));
}

#[test]
fn validation_and_listing_do_not_execute() {
    let mut case = case();
    case["command"] = json!(["bash", "-c", "exit 42"]);
    let root = fixture(case);
    assert!(success(&cli(root.path(), &["validate"])).contains("1 manifest"));
    assert!(
        success(&cli(root.path(), &["list"]))
            .contains("composite/sample/example: Example scenario")
    );
}

#[test]
fn action_selection_includes_owned_helpers_and_deletion_removes_them() {
    let root = fixture(case());
    for name in ["composite/sample/scripts/private", "composite/other"] {
        let directory = root.path().join(name);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("action.yml"), "---\nname: fixture\n").unwrap();
        fs::write(
            directory.join("test.yaml"),
            serde_json::to_string(&json!({"version": 1, "tests": [case()]})).unwrap(),
        )
        .unwrap();
    }
    // Repository fixtures outside the public collection are not action suites.
    let unrelated = root.path().join("tests/fixtures/action");
    fs::create_dir_all(&unrelated).unwrap();
    fs::write(unrelated.join("action.yml"), "---\nname: fixture\n").unwrap();
    assert!(success(&cli(root.path(), &["validate"])).contains("3 manifest"));
    assert!(
        success(&cli(root.path(), &["run", "composite/sample"])).contains("2 passed; 0 failed")
    );
    assert!(
        success(&cli(
            root.path(),
            &["run", "composite/sample/scripts/private"]
        ))
        .contains("1 passed; 0 failed")
    );
    fs::remove_dir_all(root.path().join("composite/sample")).unwrap();
    let listed = success(&cli(root.path(), &["list"]));
    assert!(listed.contains("composite/other/example"));
    assert!(!listed.contains("composite/sample"));
    assert!(success(&cli(root.path(), &["validate"])).contains("1 manifest"));
}

#[test]
fn private_helpers_require_manifests_and_do_not_follow_symlinks() {
    let root = fixture(case());
    let scripts = root.path().join("composite/sample/scripts");
    fs::create_dir_all(scripts.join("private")).unwrap();
    fs::write(scripts.join("private/action.yml"), "---\nname: private\n").unwrap();
    failure(
        &cli(root.path(), &["validate", "composite/sample"]),
        "composite/sample/scripts/private has no test.yaml",
    );
    fs::remove_dir_all(scripts.join("private")).unwrap();
    std::os::unix::fs::symlink(root.path().join("composite/sample"), scripts.join("loop")).unwrap();
    assert!(success(&cli(root.path(), &["validate"])).contains("1 manifest"));
}

#[test]
fn setup_nix_runs_real_script() {
    let output = cli(&repository(), &["run", "composite/setup-nix"]);
    assert!(success(&output).contains("6 passed; 0 failed"));
    assert!(
        success(&cli(
            &repository(),
            &[
                "run",
                "composite/setup-nix",
                "--case",
                "preserve-broken-nix-failure"
            ]
        ))
        .contains("1 passed; 0 failed")
    );
}

#[test]
fn empty_discovery_and_unknown_selections_fail() {
    let root = fixture(case());
    failure(
        &cli(
            root.path(),
            &["run", "composite/sample", "--case", "missing"],
        ),
        "zero tests",
    );
    failure(&cli(root.path(), &["run", "missing"]), "no test.yaml");
    fs::remove_file(root.path().join("composite/sample/test.yaml")).unwrap();
    failure(&cli(root.path(), &["run"]), "has no test.yaml");
    fs::remove_file(root.path().join("composite/sample/action.yml")).unwrap();
    failure(&cli(root.path(), &["run"]), "no action test.yaml");
}

#[test]
fn schema_rejects_unknown_fields_types_versions_and_empty_cases() {
    let root = fixture(case());
    for invalid in [
        json!({"version": 2, "tests": [case()]}),
        json!({"version": 1, "tests": []}),
        json!({"version": 1, "tests": [case()], "typo": true}),
        json!({"version": 1, "tests": [{"id":"bad"}]}),
    ] {
        write_manifest(root.path(), invalid);
        failure(&cli(root.path(), &["validate"]), "test.yaml");
    }
    let mut invalid = case();
    invalid["expect"]["exit"] = json!("0");
    write_manifest(root.path(), json!({"version":1,"tests":[invalid]}));
    failure(&cli(root.path(), &["validate"]), "/tests/0/expect/exit");
}

#[test]
fn duplicate_ids_and_reserved_environment_fail() {
    let root = fixture(case());
    write_manifest(root.path(), json!({"version":1,"tests":[case(), case()]}));
    failure(&cli(root.path(), &["validate"]), "duplicate case id");
    let mut invalid = case();
    invalid["env"] = json!({"PATH":"/bin"});
    write_manifest(root.path(), json!({"version":1,"tests":[invalid]}));
    failure(
        &cli(root.path(), &["validate"]),
        "reserved environment variable",
    );
}

#[test]
fn traversal_is_rejected() {
    let mut invalid = case();
    invalid["files"] = json!({"../escape":"bad"});
    let root = fixture(invalid);
    failure(&cli(root.path(), &["validate"]), "without traversal");
}

#[test]
fn assertion_failure_reports_case_and_expected_actual() {
    let mut wrong = case();
    wrong["expect"]["stdout"] = json!("missing");
    let root = fixture(wrong);
    failure(&cli(root.path(), &["run"]), "FAIL composite/sample/example");
    failure(
        &cli(root.path(), &["run"]),
        "expected \"missing\", actual \"\"",
    );
}

#[test]
fn mocks_preserve_arguments_outputs_and_failure_codes() {
    let mut mock = case();
    mock["command"] = json!(["bash", "-c", "nix 'a b' '$(touch injected)'"]);
    mock["commands"] = json!({"nix":[{"args":["a b","$(touch injected)"],"stdout":"out","stderr":"err","exit":42}]});
    mock["expect"] = json!({"exit":42,"stdout":"out","stderr":"err","calls":[{"command":"nix","args":["a b","$(touch injected)"]}],"files":{"injected":null}});
    let root = fixture(mock);
    success(&cli(root.path(), &["run"]));
}

#[test]
fn unexpected_mock_calls_fail_even_when_script_swallows_exit() {
    let mut mock = case();
    mock["command"] = json!(["bash", "-c", "nix unexpected || true"]);
    mock["commands"] = json!({"nix":[{"args":["--version"],"exit":0}]});
    mock["expect"]["calls"] = json!([{"command":"nix","args":["unexpected"]}]);
    let root = fixture(mock);
    failure(&cli(root.path(), &["run"]), "unexpected mock call");
}

#[test]
fn wrong_call_order_and_extra_calls_fail() {
    let mut mock = case();
    mock["command"] = json!(["bash", "-c", "nix --version; nix --version"]);
    mock["commands"] = json!({"nix":[{"args":["--version"],"exit":0}]});
    mock["expect"]["calls"] = json!([{"command":"nix","args":["--version"]}]);
    let root = fixture(mock);
    failure(&cli(root.path(), &["run"]), "calls: expected");
}

#[test]
fn github_files_and_fixture_changes_are_checked() {
    let mut outputs = case();
    outputs["files"] = json!({"input":"before"});
    outputs["command"] = json!([
        "bash",
        "-c",
        "printf 'value<<END\nline1\nline2\nEND\n' >>\"$GITHUB_OUTPUT\"; printf 'KEY=value\n' >>\"$GITHUB_ENV\"; printf '/fixture/bin\n' >>\"$GITHUB_PATH\"; printf after >input"
    ]);
    outputs["expect"] = json!({"exit":0,"calls":[],"github-output":{"value":"line1\nline2"},"github-env":{"KEY":"value"},"github-path":["/fixture/bin"],"files":{"input":"after","absent":null}});
    let root = fixture(outputs);
    success(&cli(root.path(), &["run"]));
    assert!(!root.path().join("input").exists());
}

#[test]
fn child_environment_does_not_inherit_ambient_values() {
    let mut clean = case();
    clean["command"] = json!([
        "bash",
        "-c",
        "printf '%s' \"${TACT_AMBIENT-unset}\"; if command -v nix; then exit 1; fi"
    ]);
    clean["expect"]["stdout"] = json!("unset");
    let root = fixture(clean);
    let output = Command::new(env!("CARGO_BIN_EXE_tact"))
        .env("TACT_AMBIENT", "must-not-leak")
        .arg("--root")
        .arg(root.path())
        .arg("run")
        .output()
        .unwrap();
    success(&output);
}

#[test]
fn timeout_fails_and_removes_fixture() {
    let mut hanging = case();
    hanging["timeout-seconds"] = json!(1);
    hanging["command"] = json!(["bash", "-c", "bash -c 'while :; do :; done' & wait"]);
    let root = fixture(hanging);
    failure(&cli(root.path(), &["run"]), "timed out after 1 seconds");
    assert_eq!(
        fs::read_dir(root.path().join(".tars/scratch/tact"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn every_action_has_a_valid_passing_manifest() {
    assert!(success(&cli(&repository(), &["validate"])).contains("8 manifest"));
    assert!(success(&cli(&repository(), &["run"])).contains("0 failed"));
    success(&cli(&repository(), &["check", "metadata"]));
}

#[test]
fn fixture_paths_and_forwarded_calls_preserve_context() {
    let mut scenario = case();
    scenario["command"] = json!(["bash", "-c", "cd nested; devenv bash -c 'tool --version'"]);
    scenario["files"] = json!({"nested/marker":"fixture"});
    scenario["env"] = json!({"PROFILE":"custom"});
    scenario["commands"] = json!({
        "devenv":[{"args":["bash","-c","tool --version"],"exit":0,"forward":0,"prepend-path":["${workspace}/package/bin"]}],
        "tool":[{"args":["--version"],"exit":0,"stdout":"declared\n"}]
    });
    scenario["command-paths"] = json!({"tool":["${workspace}/package/bin/tool"]});
    scenario["expect"] = json!({"exit":0,"stdout":"declared\n","calls":[
        {"command":"devenv","args":["bash","-c","tool --version"],"cwd":"${workspace}/nested","env":{"PROFILE":"custom"}},
        {"command":"tool","args":["--version"]}
    ]});
    let root = fixture(scenario);
    success(&cli(root.path(), &["run"]));
}

#[test]
fn ci_cache_evidence_survives_seed_and_rejects_wrong_run() {
    let root = fixture(case());
    let vars = [
        "CARGO_HOME",
        "CARGO_TARGET_DIR",
        "UV_CACHE_DIR",
        "PIP_CACHE_DIR",
        "BUN_INSTALL_CACHE_DIR",
        "TRIVY_CACHE_DIR",
    ];
    let invoke = |task: &str, id: &str| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tact"));
        command
            .arg("--root")
            .arg(root.path())
            .args(["ci", task])
            .env("GITHUB_RUN_ID", id);
        for name in vars {
            command.env(name, root.path().join(name));
        }
        command.output().unwrap()
    };
    success(&invoke("seed-cache", "fixture-run"));
    success(&invoke("verify-cache", "fixture-run"));
    failure(
        &invoke("verify-cache", "different-run"),
        "cache evidence mismatch",
    );
}

#[test]
fn ci_cache_hits_require_the_requested_backend_and_every_archive() {
    let root = fixture(case());
    let invoke = |expected: &str, actual_backend: &str, trivy: &str| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tact"));
        command
            .arg("--root")
            .arg(root.path())
            .args([
                "ci",
                "verify-hits",
                "--backend",
                "s3",
                "--expected",
                expected,
            ])
            .env("BACKEND", actual_backend);
        for name in ["CARGO", "CARGO_TARGET", "UV", "PIP", "BUN"] {
            command.env(name, expected);
        }
        command.env("TRIVY", trivy).output().unwrap()
    };
    success(&invoke("false", "s3", "false"));
    success(&invoke("true", "s3", "true"));
    failure(
        &invoke("true", "github", "true"),
        "expected s3 cache backend",
    );
    failure(
        &invoke("true", "s3", "false"),
        "TRIVY: expected exact-hit=true",
    );
}

#[test]
fn s3_fixture_reset_preserves_other_runs() {
    let root = fixture(case());
    for name in ["devenv.nix", "devenv.yaml", "devenv.lock"] {
        fs::write(root.path().join(name), "fixture").unwrap();
    }
    let temp = root.path().join("runner-temp");
    for name in ["tact-s3-123-1", "tact-s3-456-1"] {
        fs::create_dir_all(temp.join(name)).unwrap();
        fs::write(temp.join(name).join("proof"), "old").unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_tact"))
        .arg("--root")
        .arg(root.path())
        .args(["ci", "prepare-cache", "--reset-s3-fixture"])
        .env("GITHUB_RUN_ID", "123")
        .env("GITHUB_RUN_ATTEMPT", "1")
        .env("RUNNER_TEMP", &temp)
        .output()
        .unwrap();
    success(&output);
    assert!(!temp.join("tact-s3-123-1").exists());
    assert!(temp.join("tact-s3-456-1/proof").is_file());
}
