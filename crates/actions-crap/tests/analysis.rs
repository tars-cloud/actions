use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct Fixture {
    directory: tempfile::TempDir,
    project: PathBuf,
    revision: String,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("consumer");
        fs::create_dir_all(project.join("src")).unwrap();
        fs::create_dir_all(project.join(".cargo")).unwrap();
        fs::create_dir(directory.path().join("bin")).unwrap();
        fs::write(
            project.join("Cargo.toml"),
            "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n",
        )
        .unwrap();
        fs::write(project.join("Cargo.lock"), "# fixture lock\n").unwrap();
        fs::write(project.join("src/lib.rs"), "pub fn choose() {}\n").unwrap();
        fs::write(
            project.join(".cargo/config.toml"),
            "[build]\nrustflags = ['--cfg', 'selected']\n",
        )
        .unwrap();
        fs::write(
            project.join(".cargo-crap.toml"),
            "try-weight = 0.5\ntop = 1\nmin = 1000\n",
        )
        .unwrap();
        for args in [
            vec!["init", "-b", "trunk"],
            vec!["config", "user.name", "Fixture"],
            vec!["config", "user.email", "fixture@example.invalid"],
            vec!["add", "."],
            vec!["commit", "-m", "baseline"],
        ] {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(&project)
                    .output()
                    .unwrap()
                    .status
                    .success()
            );
        }
        let revision = String::from_utf8(
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&project)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_owned();
        let report = json!({"version":"0.6.1", "entries":[{"file":"src/lib.rs","function":"choose","line":1,"cyclomatic":2.0,"coverage":100.0,"crap":2.0}],"diagnostics":{"analyzed_files":1,"lcov_files":1,"matched_files":1,"source_only":{"count":0,"examples":[]},"lcov_only":{"count":0,"examples":[]}}});
        fs::write(directory.path().join("absolute.json"), report.to_string()).unwrap();
        fs::write(
            directory.path().join("badge.json"),
            json!({"schemaVersion":1,"label":"CRAP > 30","message":"passing","color":"brightgreen"}).to_string(),
        )
        .unwrap();
        let mut delta = report;
        delta["removed"] = json!([]);
        delta["entries"][0]["status"] = json!("regressed");
        delta["entries"][0]["baseline_crap"] = json!(1.0);
        delta["entries"][0]["delta"] = json!(1.0);
        fs::write(directory.path().join("delta.json"), delta.to_string()).unwrap();
        let script = directory.path().join("bin/cargo");
        let bash = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|directory| directory.join("bash"))
            .find(|path| path.is_file())
            .expect("selected Bash for fixture scripts");
        let body = r#"set -euo pipefail
printf '%s\n' "$*" >> "$FIXTURE_DIRECTORY/calls"
if [[ ${2:-} == --version ]]; then
    printf '%s 0.6.1\n' "$1"
    exit 0
fi
if [[ $1 != crap ]]; then
    [[ ${FAIL_COVERAGE:-} != true ]] || exit 7
    output=''
    for ((i=1; i<=$#; i++)); do
        case ${!i} in
            --output-path) ((i+=1)); output=${!i} ;;
            --output-dir) ((i+=1)); output=${!i}/lcov.info ;;
        esac
    done
    printf 'SF:%s/src/lib.rs\nDA:1,1\nend_of_record\n' "$PWD" > "$output"
    exit 0
fi
format=''; output=''; report=absolute
for ((i=2; i<=$#; i++)); do
    case ${!i} in
        --format) ((i+=1)); format=${!i} ;;
        --output) ((i+=1)); output=${!i} ;;
        --baseline) report=delta ;;
    esac
done
if [[ $format == shields ]]; then
    cat "$FIXTURE_DIRECTORY/badge.json" > "$output"
else
    jq --arg root "$PWD" '.entries |= map(.file = ($root + "/" + .file))' "$FIXTURE_DIRECTORY/$report.json" > "$output"
fi
"#;
        // Nix build sandboxes have no /usr/bin/env interpreter.
        fs::write(&script, format!("#!{}\n{body}", bash.display())).unwrap();
        fs::set_permissions(script, fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            directory,
            project,
            revision,
        }
    }

    fn command(&self, operation: &str, backend: &str, baseline: &Path) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_actions-crap"));
        let path = std::env::join_paths(
            std::iter::once(self.directory.path().join("bin"))
                .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
        )
        .unwrap();
        command
            .current_dir(&self.project)
            .env_clear()
            .env("PATH", path)
            .env("HOME", self.directory.path())
            .env("FIXTURE_DIRECTORY", self.directory.path())
            .env("RUNNER_TEMP", self.directory.path())
            .env("CRAP_COMMIT", &self.revision)
            .env("CRAP_BASELINE_COMMIT", &self.revision)
            .env("CRAP_OPERATION", operation)
            .env("CRAP_COVERAGE_TOOL", backend)
            .env("CRAP_BASELINE_DIRECTORY", baseline);
        if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
            command.env("LLVM_PROFILE_FILE", profile);
        }
        command
    }

    fn analyze(&self, operation: &str, backend: &str, baseline: &Path) -> PathBuf {
        let outputs = self.directory.path().join("outputs");
        fs::write(&outputs, "").unwrap();
        let output = self
            .command(operation, backend, baseline)
            .env("GITHUB_OUTPUT", &outputs)
            .env("GITHUB_STEP_SUMMARY", self.directory.path().join("summary"))
            .output()
            .unwrap();
        success(&output);
        let text = fs::read_to_string(outputs).unwrap();
        PathBuf::from(
            text.lines()
                .find_map(|line| line.strip_prefix("report-directory="))
                .unwrap(),
        )
    }
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn read(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
fn badges_preserve_upstream_counts_and_use_orange_for_minor_debt() {
    for (count, upstream_color, color) in [
        (0, "brightgreen", "brightgreen"),
        (1, "yellow", "orange"),
        (5, "yellow", "orange"),
        (6, "red", "red"),
        (18, "red", "red"),
    ] {
        let fixture = Fixture::new();
        let report_file = fixture.directory.path().join("absolute.json");
        let mut report = read(&report_file);
        let entry = report["entries"][0].clone();
        let mut entries = vec![entry.clone()];
        entries[0]["crap"] = json!(30.0);
        for index in 0..count {
            let mut offender = entry.clone();
            offender["function"] = json!(format!("offender_{index}"));
            offender["crap"] = json!(31.0);
            entries.push(offender);
        }
        report["entries"] = json!(entries);
        fs::write(report_file, report.to_string()).unwrap();
        let message = if count == 0 {
            "passing".to_owned()
        } else {
            format!("{count} crappy")
        };
        let badge =
            json!({"schemaVersion":1,"label":"CRAP > 30","message":message,"color":upstream_color});
        fs::write(
            fixture.directory.path().join("badge.json"),
            badge.to_string(),
        )
        .unwrap();
        let output = fixture.analyze("measure", "llvm-cov", Path::new(""));
        let mut expected = badge;
        expected["color"] = json!(color);
        assert_eq!(read(output.join("crap-badge.json")), expected);
        assert_eq!(
            read(output.join("current-reports/crap-badge.json")),
            expected
        );
        let result = read(output.join("result.json"));
        assert_eq!(result["existing_debt"], count);
        assert_eq!(result["quality"], "pass");
    }
}

#[test]
fn records_accepted_debt_and_compares_exact_or_fresh_baselines() {
    let fixture = Fixture::new();
    let baseline = fixture.analyze("measure", "llvm-cov", Path::new(""));
    let metadata = read(baseline.join("metadata.json"));
    assert_eq!(metadata["complete"], true);
    assert_eq!(metadata["profile"]["packages"], json!([]));
    assert!(metadata["profile"]["declarations"][".cargo/config.toml"].is_string());
    assert_eq!(
        read(baseline.join("baseline.json"))["entries"][0]["file"],
        "src/lib.rs"
    );
    assert_eq!(read(baseline.join("result.json"))["quality"], "pass");
    assert!(
        fs::read_to_string(baseline.join("current/.cargo-crap.toml"))
            .unwrap()
            .contains("try-weight = 0.5")
    );
    assert!(
        !fs::read_to_string(baseline.join("current/.cargo-crap.toml"))
            .unwrap()
            .contains("top")
    );
    for (cached, source) in [
        (baseline.as_path(), "artifact"),
        (Path::new("missing"), "fresh"),
    ] {
        let comparison = fixture.analyze("compare", "llvm-cov", cached);
        let result = read(comparison.join("result.json"));
        assert_eq!(result["quality"], "fail");
        assert_eq!(result["regressed"], 1);
        assert_eq!(result["baseline_source"], source);
        assert!(
            fs::read_to_string(comparison.join("summary.md"))
                .unwrap()
                .contains("- regressed: `choose` in `src/lib.rs`: 1.0 → 2.0.")
        );
    }
    let mut incompatible = metadata;
    incompatible["profile"]["action"] = json!("other-revision");
    fs::write(baseline.join("metadata.json"), incompatible.to_string()).unwrap();
    let comparison = fixture.analyze("compare", "llvm-cov", &baseline);
    assert_eq!(
        read(comparison.join("result.json"))["baseline_source"],
        "fresh"
    );
    assert!(
        fs::read_to_string(fixture.project.join(".cargo-crap.toml"))
            .unwrap()
            .contains("top = 1")
    );
}

#[test]
fn tarpaulin_scope_and_coverage_failures_preserve_the_contract() {
    let fixture = Fixture::new();
    let output = fixture
        .command("measure", "tarpaulin", Path::new(""))
        .env("CRAP_PACKAGES", "[\"fixture\"]")
        .env("CRAP_FEATURES", "[\"extra\",\"literal$(value)\"]")
        .output()
        .unwrap();
    success(&output);
    let calls = fs::read_to_string(fixture.directory.path().join("calls")).unwrap();
    assert!(calls.contains("tarpaulin --locked --packages fixture --features extra,literal$(value) --ignore-config --engine Llvm --out Lcov"));
    assert!(calls.contains("--package fixture"));
    let failed = fixture
        .command("measure", "llvm-cov", Path::new(""))
        .env("FAIL_COVERAGE", "true")
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("no score gate evaluated"));
    for args in [
        vec!["resolve-tool"],
        vec!["validate-smoke"],
        vec!["unexpected"],
    ] {
        assert!(
            !fixture
                .command("measure", "llvm-cov", Path::new(""))
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}
