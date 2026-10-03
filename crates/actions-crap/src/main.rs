use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Component, Path, PathBuf},
    process::Command,
};
use toml_edit::{DocumentMut, value};

fn setting(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_owned())
}

fn run(cwd: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("start {program}"))?;
    ensure!(
        output.status.success(),
        "{program} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn execute(cwd: &Path, program: &str, args: &[String]) -> Result<()> {
    let status = Command::new(program).args(args).current_dir(cwd).status()?;
    ensure!(
        status.success(),
        "{program} failed ({status}); no baseline was promoted"
    );
    Ok(())
}

fn sha(value: &str) -> Result<()> {
    ensure!(
        value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit()),
        "expected a full commit SHA"
    );
    Ok(())
}

fn safe_relative(path: &Path) -> Result<()> {
    ensure!(
        !path.is_absolute()
            && path
                .components()
                .all(|c| matches!(c, Component::Normal(_) | Component::CurDir)),
        "path must stay inside the checkout"
    );
    Ok(())
}

fn locked_tool_version(metadata: &Value) -> Result<&str> {
    let members = metadata["workspace_members"]
        .as_array()
        .context("Cargo metadata workspace members")?;
    let packages = metadata["packages"]
        .as_array()
        .context("Cargo metadata packages")?;
    let nodes = metadata["resolve"]["nodes"]
        .as_array()
        .context("Cargo metadata resolved dependencies")?;
    let mut tools = BTreeSet::new();
    for node in nodes.iter().filter(|node| members.contains(&node["id"])) {
        for dependency in node["deps"]
            .as_array()
            .context("resolved workspace dependencies")?
        {
            for package in packages.iter().filter(|package| {
                package["id"] == dependency["pkg"] && package["name"] == "cargo-crap"
            }) {
                ensure!(
                    package["source"] == "registry+https://github.com/rust-lang/crates.io-index",
                    "Cargo-declared cargo-crap must come from crates.io; provide Git/path tools through the selected environment"
                );
                tools.insert(
                    package["version"]
                        .as_str()
                        .context("locked cargo-crap version")?,
                );
            }
        }
    }
    ensure!(
        tools.len() == 1,
        "Declare one cargo-crap version as a workspace member dependency or dev-dependency and update Cargo.lock; workspace.dependencies must be inherited by a member"
    );
    let version = tools
        .into_iter()
        .next()
        .context("locked cargo-crap version")?;
    ensure!(
        version == "0.6.1",
        "Analysis contract v1 requires cargo-crap 0.6.1; Cargo.lock resolves {version}"
    );
    Ok(version)
}

fn effective_config(input: &str, threshold: f64, epsilon: f64) -> Result<String> {
    let mut config = input.parse::<DocumentMut>()?;
    // These options affect the population of scored functions, not just presentation.
    for key in [
        "top",
        "min",
        "duplicates",
        "show_unchanged",
        "show-unchanged",
    ] {
        config.remove(key);
    }
    config["fail-above"] = value(false);
    config["fail-regression"] = value(false);
    config["missing"] = value("pessimistic");
    config["sort"] = value("file");
    config["threshold"] = value(threshold);
    config["epsilon"] = value(epsilon);
    Ok(config.to_string())
}

fn validate(report: &Value, delta: bool) -> Result<()> {
    let schema: Value = serde_json::from_str(if delta {
        include_str!("../../../composite/cargo-crap/scripts/schemas/delta-v2.json")
    } else {
        include_str!("../../../composite/cargo-crap/scripts/schemas/report-v1.json")
    })?;
    let validator = jsonschema::validator_for(&schema)?;
    ensure!(
        validator.is_valid(report),
        "invalid cargo-crap report: {:?}",
        validator
            .iter_errors(report)
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
    );
    ensure!(
        report["version"] == "0.6.1",
        "cargo-crap 0.6.1 is required by analysis contract v1"
    );
    ensure!(
        !report["entries"].as_array().context("entries")?.is_empty(),
        "analysis contains no functions"
    );
    ensure!(
        report["diagnostics"]["matched_files"].as_u64().unwrap_or(0) > 0,
        "no source/LCOV overlap; check coverage scope and source paths"
    );
    Ok(())
}

fn normalize(report: &mut Value, root: &Path) -> Result<()> {
    fn path(v: &mut Value, root: &Path) -> Result<()> {
        let original = Path::new(v.as_str().context("source path")?);
        let relative = if original.is_absolute() {
            original
                .strip_prefix(root)
                .context("report path outside analysis checkout")?
        } else {
            original
        };
        safe_relative(relative)?;
        *v = json!(relative.to_str().context("UTF-8 source path")?);
        Ok(())
    }
    for key in ["entries", "removed"] {
        if let Some(entries) = report.get_mut(key).and_then(Value::as_array_mut) {
            for entry in entries.iter_mut() {
                path(&mut entry["file"], root)?;
                if let Some(previous) = entry.get_mut("previous_file") {
                    path(previous, root)?;
                }
            }
            entries.sort_by_key(|e| {
                (
                    e["file"].to_string(),
                    e["function"].to_string(),
                    e["line"].as_u64(),
                )
            });
        }
    }
    for side in ["source_only", "lcov_only"] {
        if let Some(paths) = report["diagnostics"][side]["examples"].as_array_mut() {
            for entry in paths {
                let original = Path::new(entry.as_str().context("diagnostic path")?);
                if original.is_absolute() && !original.starts_with(root) {
                    *entry = json!(format!(
                        "<external>/{}",
                        original
                            .file_name()
                            .context("external source name")?
                            .to_string_lossy()
                    ));
                } else {
                    path(entry, root)?;
                }
            }
        }
    }
    Ok(())
}

fn verdict(delta: &Value, threshold: f64, epsilon: f64) -> Result<Value> {
    let entries = delta["entries"].as_array().context("delta entries")?;
    let mut regressed = 0;
    let mut improved = 0;
    let mut new = 0;
    let mut new_above = 0;
    let mut debt = 0;
    for entry in entries {
        let score = entry["crap"].as_f64().context("score")?;
        match entry["status"].as_str().context("status")? {
            "new" => {
                new += 1;
                if score > threshold {
                    new_above += 1;
                }
            }
            "regressed" => {
                ensure!(
                    entry["delta"].as_f64().context("regression delta")? > epsilon,
                    "inconsistent regression classification"
                );
                regressed += 1;
            }
            "improved" => improved += 1,
            "moved" | "unchanged" => {}
            other => bail!("unsupported delta status: {other}"),
        }
        if score > threshold && entry["status"] != "new" {
            debt += 1;
        }
    }
    Ok(
        json!({"quality": if regressed + new_above == 0 {"pass"} else {"fail"}, "regressed":regressed, "improved":improved, "new":new, "new_above":new_above, "existing_debt":debt, "removed":delta["removed"].as_array().context("removed")?.len()}),
    )
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}

fn read_json(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn digest(path: &Path) -> Result<String> {
    Ok(run(
        Path::new("/"),
        "sha256sum",
        &[path.to_str().context("hash path")?],
    )?
    .split_whitespace()
    .next()
    .context("hash")?
    .to_owned())
}

fn config_from(project: &Path, root: &Path) -> Result<String> {
    let mut directory = project;
    loop {
        let path = directory.join(".cargo-crap.toml");
        if path.exists() {
            ensure!(
                path.canonicalize()?.starts_with(root),
                "scoring config escapes checkout"
            );
            return Ok(fs::read_to_string(path)?);
        }
        if directory == root {
            return Ok(String::new());
        }
        directory = directory
            .parent()
            .context("configuration outside repository")?;
    }
}

fn cargo_configs(project: &Path, root: &Path) -> Result<BTreeMap<PathBuf, String>> {
    let mut files = BTreeMap::new();
    let mut directory = project;
    loop {
        for name in [".cargo/config", ".cargo/config.toml"] {
            let file = directory.join(name);
            if file.is_file() {
                ensure!(
                    file.canonicalize()?.starts_with(root),
                    "Cargo config escapes checkout"
                );
                files.insert(
                    file.strip_prefix(root)?.to_path_buf(),
                    fs::read_to_string(file)?,
                );
            }
        }
        if directory == root {
            break;
        }
        directory = directory
            .parent()
            .context("Cargo configuration outside repository")?;
    }
    Ok(files)
}

fn apply_cargo_configs(
    directory: &Path,
    relative: &Path,
    selected: &BTreeMap<PathBuf, String>,
) -> Result<()> {
    let mut project = directory.join(relative);
    loop {
        let cargo = project.join(".cargo");
        if fs::symlink_metadata(&cargo).is_ok() {
            ensure!(
                !fs::symlink_metadata(&cargo)?.file_type().is_symlink(),
                "Cargo config directory must not be a symlink"
            );
            for name in ["config", "config.toml"] {
                let file = cargo.join(name);
                if fs::symlink_metadata(&file).is_ok() {
                    fs::remove_file(file)?;
                }
            }
        }
        if project == directory {
            break;
        }
        project = project
            .parent()
            .context("Cargo config outside checkout")?
            .to_path_buf();
    }
    for (file, content) in selected {
        let path = directory.join(file);
        fs::create_dir_all(path.parent().context("Cargo config parent")?)?;
        fs::write(path, content)?;
    }
    Ok(())
}

fn write_scoring_config(directory: &Path, config: &str) -> Result<()> {
    let file = directory.join(".cargo-crap.toml");
    if fs::symlink_metadata(&file).is_ok() {
        fs::remove_file(&file)?;
    }
    fs::write(file, config)?;
    Ok(())
}

struct Analysis<'a> {
    root: &'a Path,
    relative: &'a Path,
    output: &'a Path,
    config: &'a str,
    cargo_configs: BTreeMap<PathBuf, String>,
    backend: &'a str,
    threshold: f64,
    epsilon: f64,
    packages: Vec<String>,
    features: Vec<String>,
}

impl Analysis<'_> {
    fn coverage(&self, project: &Path, label: &str, reports: &Path) -> Result<()> {
        let lcov = reports.join("lcov.info");
        let mut scope = if self.packages.is_empty() {
            vec!["--workspace".to_owned()]
        } else {
            self.packages
                .iter()
                .flat_map(|p| {
                    [
                        if self.backend == "tarpaulin" {
                            "--packages".into()
                        } else {
                            "--package".into()
                        },
                        p.clone(),
                    ]
                })
                .collect()
        };
        if !self.features.is_empty() {
            scope.extend(["--features".into(), self.features.join(",")]);
        }
        let mut args = vec![self.backend.into(), "--locked".into()];
        args.extend(scope);
        if self.backend == "llvm-cov" {
            args.extend([
                "--lcov".into(),
                "--output-path".into(),
                lcov.display().to_string(),
            ]);
        } else {
            args.extend([
                "--ignore-config".into(),
                "--engine".into(),
                "Llvm".into(),
                "--out".into(),
                "Lcov".into(),
                "--output-dir".into(),
                reports.display().to_string(),
            ]);
        }
        let status = Command::new("cargo")
            .args(&args)
            .current_dir(project)
            .env(
                "CARGO_TARGET_DIR",
                self.output.join(format!("{label}-target")),
            )
            .env(
                "CARGO_LLVM_COV_TARGET_DIR",
                self.output.join(format!("{label}-target")),
            )
            .env("CARGO_LLVM_COV_SETUP", "no")
            .status()?;
        ensure!(
            status.success(),
            "coverage failed ({status}); no score gate evaluated"
        );
        let trace = fs::read_to_string(&lcov).context("coverage backend did not produce LCOV")?;
        ensure!(
            trace.contains("SF:") && trace.contains("DA:") && trace.contains("end_of_record"),
            "empty or malformed LCOV"
        );
        Ok(())
    }

    fn measure(&self, revision: &str, label: &str, baseline: Option<&Path>) -> Result<Value> {
        sha(revision)?;
        let directory = self.output.join(label);
        execute(
            self.root,
            "git",
            &[
                "clone".into(),
                "--quiet".into(),
                "--no-hardlinks".into(),
                "--no-checkout".into(),
                self.root.display().to_string(),
                directory.display().to_string(),
            ],
        )?;
        execute(
            &directory,
            "git",
            &[
                "checkout".into(),
                "--quiet".into(),
                "--detach".into(),
                revision.into(),
            ],
        )?;
        let project = directory.join(self.relative).canonicalize()?;
        ensure!(
            project.starts_with(&directory),
            "working-directory escapes analysis checkout"
        );
        apply_cargo_configs(&directory, self.relative, &self.cargo_configs)?;
        write_scoring_config(&directory, self.config)?;
        write_scoring_config(&project, self.config)?;
        let reports = self.output.join(format!("{label}-reports"));
        fs::create_dir_all(&reports)?;
        let lcov = reports.join("lcov.info");
        self.coverage(&project, label, &reports)?;
        let mut scoring = vec![
            "crap".into(),
            "--lcov".into(),
            lcov.display().to_string(),
            "--threshold".into(),
            self.threshold.to_string(),
            "--epsilon".into(),
            self.epsilon.to_string(),
        ];
        if self.packages.is_empty() {
            scoring.push("--workspace".into());
        }
        for package in &self.packages {
            scoring.extend(["--package".into(), package.clone()]);
        }
        let absolute_file = reports.join("absolute.json");
        let render = |format: &str, path: &Path, base: Option<&Path>| -> Result<()> {
            let mut args = scoring.clone();
            args.extend([
                "--format".into(),
                format.into(),
                "--output".into(),
                path.display().to_string(),
            ]);
            if let Some(base) = base {
                args.extend(["--baseline".into(), base.display().to_string()]);
            }
            execute(&project, "cargo", &args)
        };
        render("json", &absolute_file, None)?;
        let mut absolute = read_json(&absolute_file)?;
        validate(&absolute, false)?;
        normalize(&mut absolute, &directory)?;
        write_json(&absolute_file, &absolute)?;
        let badge_file = reports.join("crap-badge.json");
        render("shields", &badge_file, None)?;
        let mut badge = read_json(&badge_file)?;
        if badge["color"] == "yellow" {
            badge["color"] = json!("orange");
        }
        write_json(&badge_file, &badge)?;
        let result = if let Some(base) = baseline {
            let delta_file = reports.join("delta.json");
            render("json", &delta_file, Some(base))?;
            let mut delta = read_json(&delta_file)?;
            validate(&delta, true)?;
            normalize(&mut delta, &directory)?;
            write_json(&delta_file, &delta)?;
            verdict(&delta, self.threshold, self.epsilon)?
        } else {
            json!({"quality":"pass", "existing_debt": absolute["entries"].as_array().context("entries")?.iter().filter(|e| e["crap"].as_f64().unwrap_or(0.0) > self.threshold).count()})
        };
        Ok(result)
    }
}

fn analyze() -> Result<()> {
    let project = env::current_dir()?.canonicalize()?;
    let root =
        PathBuf::from(run(&project, "git", &["rev-parse", "--show-toplevel"])?).canonicalize()?;
    let relative = project.strip_prefix(&root)?;
    let revision = setting("CRAP_COMMIT", "");
    sha(&revision)?;
    ensure!(
        run(&root, "git", &["rev-parse", "HEAD"])? == revision,
        "checkout does not match commit-sha"
    );
    let baseline_sha = setting("CRAP_BASELINE_COMMIT", "");
    let operation = setting("CRAP_OPERATION", "measure");
    ensure!(
        matches!(operation.as_str(), "measure" | "compare"),
        "operation must be measure or compare"
    );
    let threshold: f64 = setting("CRAP_THRESHOLD", "30").parse()?;
    let epsilon: f64 = setting("CRAP_EPSILON", "0.01").parse()?;
    ensure!(
        threshold.is_finite() && threshold >= 1.0 && epsilon.is_finite() && epsilon >= 0.0,
        "invalid threshold or epsilon"
    );
    let backend = setting("CRAP_COVERAGE_TOOL", "llvm-cov");
    ensure!(
        matches!(backend.as_str(), "llvm-cov" | "tarpaulin"),
        "unsupported coverage-tool"
    );
    let list = |name| -> Result<Vec<String>> {
        let entries: Vec<String> = serde_json::from_str(&setting(name, "[]"))?;
        ensure!(
            entries
                .iter()
                .all(|e| !e.is_empty() && !e.starts_with('-') && !e.contains(['\n', '\r'])),
            "invalid scope list"
        );
        Ok(entries)
    };
    let packages = list("CRAP_PACKAGES")?;
    let features = list("CRAP_FEATURES")?;
    let config = effective_config(&config_from(&project, &root)?, threshold, epsilon)?;
    let cargo_configurations = cargo_configs(&project, &root)?;
    let output = PathBuf::from(setting("RUNNER_TEMP", "/tmp"))
        .join(format!("cargo-crap-{}", std::process::id()));
    fs::create_dir(&output)?;
    let declarations = declarations(&project, &root, &cargo_configurations)?;
    let flags = compiler_flags();
    let profile = json!({"contract":1, "action": setting("CRAP_ACTION_REVISION", ""), "compiler":run(&project,"rustc", &["-vV"] )?, "coverage_tool": backend, "coverage_version":run(&project,"cargo", &[&backend,"--version"] )?, "cargo_crap":run(&project,"cargo", &["crap","--version"] )?, "config":config, "packages":packages,"features":features,"environment":setting("ENVIRONMENT_TYPE","devenv"), "shell":setting("FLAKE_SHELL",".#default"), "declarations":declarations,"flags": flags});
    let analysis = Analysis {
        root: &root,
        relative,
        output: &output,
        config: &config,
        cargo_configs: cargo_configurations,
        backend: &backend,
        threshold,
        epsilon,
        packages,
        features,
    };
    let baseline_source = analysis.baseline(&operation, &baseline_sha, &profile)?;
    let baseline = output.join("baseline.json");
    let mut result = analysis.measure(
        &revision,
        "current",
        (operation == "compare").then_some(baseline.as_path()),
    )?;
    fs::copy(
        output.join("current-reports/absolute.json"),
        output.join("baseline.json"),
    )?;
    fs::copy(
        output.join("current-reports/crap-badge.json"),
        output.join("crap-badge.json"),
    )?;
    let metadata = json!({"complete":true,"operation":operation,"commit":revision,"baseline_commit":baseline_sha,"baseline_source":baseline_source,"profile":profile,"baseline_hash":digest(&output.join("baseline.json"))?,"badge_hash":digest(&output.join("crap-badge.json"))?,"run_id":setting("GITHUB_RUN_ID",""),"head_commit":setting("CRAP_HEAD_COMMIT","")});
    write_json(&output.join("metadata.json"), &metadata)?;
    result["complete"] = json!(true);
    result["commit"] = json!(revision);
    result["baseline_commit"] = json!(baseline_sha);
    result["baseline_source"] = json!(baseline_source);
    write_json(&output.join("result.json"), &result)?;
    let summary = summary(
        &analysis,
        &result,
        &revision,
        &baseline_sha,
        baseline_source,
        &operation,
    )?;
    publish(&output, &result, &summary)
}

fn declarations(
    project: &Path,
    root: &Path,
    cargo_configurations: &BTreeMap<PathBuf, String>,
) -> Result<serde_json::Map<String, Value>> {
    let mut declarations = serde_json::Map::new();
    for name in [
        "devenv.nix",
        "devenv.yaml",
        "devenv.lock",
        "flake.nix",
        "flake.lock",
        "rust-toolchain.toml",
        "rust-toolchain",
        ".cargo/config.toml",
        "Cargo.lock",
    ] {
        let file = project.join(name);
        if file.is_file() {
            declarations.insert(name.into(), json!(digest(&file)?));
        }
    }
    for file in cargo_configurations.keys() {
        declarations.insert(file.display().to_string(), json!(digest(&root.join(file))?));
    }
    Ok(declarations)
}

fn compiler_flags() -> BTreeMap<String, String> {
    env::vars()
        .filter(|(key, _)| {
            [
                "RUSTFLAGS",
                "RUSTDOCFLAGS",
                "CARGO_ENCODED_RUSTFLAGS",
                "CARGO_ENCODED_RUSTDOCFLAGS",
                "CARGO_BUILD_TARGET",
                "RUSTC",
                "RUSTC_WRAPPER",
                "RUSTC_WORKSPACE_WRAPPER",
                "LLVM_COV",
                "LLVM_PROFDATA",
            ]
            .contains(&key.as_str())
                || key.starts_with("CARGO_PROFILE_")
                || (key.starts_with("CARGO_TARGET_")
                    && (key.ends_with("_RUSTFLAGS")
                        || key.ends_with("_LINKER")
                        || key.ends_with("_RUNNER")))
        })
        .collect()
}

impl Analysis<'_> {
    fn baseline(
        &self,
        operation: &str,
        baseline_sha: &str,
        profile: &Value,
    ) -> Result<&'static str> {
        let output = self.output;
        let mut baseline_source = "none";
        let baseline = output.join("baseline.json");
        if operation == "compare" {
            sha(baseline_sha)?;
            let cached = PathBuf::from(setting("CRAP_BASELINE_DIRECTORY", ""));
            let compatible = (|| -> Result<bool> {
                let metadata = read_json(&cached.join("metadata.json"))?;
                let report = read_json(&cached.join("baseline.json"))?;
                validate(&report, false)?;
                if metadata["profile"] != *profile {
                    let differences: Vec<_> = profile
                        .as_object()
                        .context("profile")?
                        .keys()
                        .filter(|key| metadata["profile"][*key] != profile[*key])
                        .collect();
                    eprintln!(
                        "Baseline profile changed: {differences:?}; measuring captured baseline source."
                    );
                }
                Ok(metadata["complete"] == true
                    && metadata["commit"] == baseline_sha
                    && metadata["profile"] == *profile
                    && metadata["baseline_hash"] == digest(&cached.join("baseline.json"))?)
            })()
            .unwrap_or_else(|error| {
                eprintln!(
                    "Baseline artifact unavailable or invalid: {error}; measuring captured baseline source."
                );
                false
            });
            if compatible {
                fs::copy(cached.join("baseline.json"), &baseline)?;
                baseline_source = "artifact";
            } else {
                self.measure(baseline_sha, "base", None)?;
                fs::copy(output.join("base-reports/absolute.json"), &baseline)?;
                baseline_source = "fresh";
            }
        }
        Ok(baseline_source)
    }
}

fn summary(
    analysis: &Analysis<'_>,
    result: &Value,
    revision: &str,
    baseline_sha: &str,
    baseline_source: &str,
    operation: &str,
) -> Result<String> {
    let threshold = analysis.threshold;
    let backend = analysis.backend;
    let output = analysis.output;
    let mut summary = format!(
        r#"CRAP quality: **{}**.

Compared commit `{}` with baseline `{}` ({}).

Regressions: {}. Improvements: {}. New functions: {}. New above {}: {}. Existing above threshold: {}.

Bypassing a failed check accepts the debt when the PR is merged.
The next valid baseline branch measurement becomes the baseline.
"#,
        result["quality"].as_str().context("quality")?,
        revision,
        baseline_sha,
        baseline_source,
        result["regressed"].as_u64().unwrap_or(0),
        result["improved"].as_u64().unwrap_or(0),
        result["new"].as_u64().unwrap_or(0),
        threshold,
        result["new_above"].as_u64().unwrap_or(0),
        result["existing_debt"].as_u64().unwrap_or(0)
    );
    let display = |text: &str| {
        text.replace(['\n', '\r'], " ")
            .replace('`', "'")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    summary.push_str(&format!("\nCoverage backend: `{backend}`. Packages: `{}`. Additional features: `{}`. Default features remain enabled.\n", display(&if analysis.packages.is_empty() {"workspace".into()} else {analysis.packages.join(", ")}), display(&analysis.features.join(", "))));
    if baseline_source == "fresh" {
        summary.push_str("\nNo compatible exact-commit artifact was available; the captured baseline was measured under this run's profile.\n");
    }
    if operation == "compare" {
        let delta = read_json(&output.join("current-reports/delta.json"))?;
        let changes: Vec<_> = delta["entries"]
            .as_array()
            .context("delta entries")?
            .iter()
            .filter(|e| {
                ["regressed", "improved", "new"].contains(&e["status"].as_str().unwrap_or(""))
            })
            .collect();
        if !changes.is_empty() {
            summary.push('\n');
            for entry in changes.iter().take(20) {
                summary.push_str(&format!(
                    "- {}: `{}` in `{}`: {} → {}.\n",
                    entry["status"].as_str().context("status")?,
                    display(entry["function"].as_str().context("function")?),
                    display(entry["file"].as_str().context("file")?),
                    if entry["baseline_crap"].is_null() {
                        "new".into()
                    } else {
                        entry["baseline_crap"].to_string()
                    },
                    entry["crap"]
                ));
            }
            if changes.len() > 20 {
                summary.push_str("\nSee the delta artifact for the remaining changed functions.\n");
            }
        }
    }
    Ok(summary)
}

fn publish(output: &Path, result: &Value, summary: &str) -> Result<()> {
    fs::write(output.join("summary.md"), summary)?;
    if let Ok(path) = env::var("GITHUB_STEP_SUMMARY") {
        use std::io::Write;
        fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)?
            .write_all(summary.as_bytes())?;
    }
    if let Ok(path) = env::var("GITHUB_OUTPUT") {
        use std::io::Write;
        writeln!(
            fs::OpenOptions::new().append(true).open(path)?,
            "complete=true\nquality={}\nreport-directory={}\nresult={}",
            result["quality"].as_str().context("quality")?,
            output.display(),
            serde_json::to_string(&result)?
        )?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "resolve-tool") {
        ensure!(
            args.len() == 2,
            "resolve-tool requires one Cargo metadata path"
        );
        let metadata = read_json(Path::new(&args[1]))?;
        println!("{}", locked_tool_version(&metadata)?);
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "validate-smoke") {
        ensure!(args.len() == 2, "validate-smoke requires one report path");
        return validate(&read_json(Path::new(&args[1]))?, false);
    }
    ensure!(args.is_empty(), "unexpected analysis arguments");
    analyze()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tool_metadata() -> Value {
        json!({
            "workspace_members": ["consumer"],
            "packages": [{
                "id": "locked-tool", "name": "cargo-crap", "version": "0.6.1",
                "source": "registry+https://github.com/rust-lang/crates.io-index"
            }],
            "resolve": {"nodes": [{"id": "consumer", "deps": [{"name": "renamed_tool", "pkg": "locked-tool"}]}]}
        })
    }

    #[test]
    fn resolves_locked_direct_tool_including_renamed_dependencies() {
        assert_eq!(locked_tool_version(&tool_metadata()).unwrap(), "0.6.1");
    }

    #[test]
    fn rejects_transitive_tools_and_unused_workspace_declarations() {
        let mut metadata = tool_metadata();
        metadata["resolve"]["nodes"][0]["id"] = json!("transitive-package");
        assert!(locked_tool_version(&metadata).is_err());
        metadata["resolve"]["nodes"] = json!([]);
        assert!(locked_tool_version(&metadata).is_err());
    }

    #[test]
    fn rejects_unsupported_or_ambiguous_versions_and_other_sources() {
        let mut metadata = tool_metadata();
        metadata["packages"][0]["version"] = json!("0.6.2");
        assert!(locked_tool_version(&metadata).is_err());
        metadata = tool_metadata();
        metadata["packages"][0]["source"] = json!("git+https://example.invalid/tool#deadbeef");
        assert!(locked_tool_version(&metadata).is_err());
        metadata["packages"][0]["source"] = Value::Null;
        assert!(locked_tool_version(&metadata).is_err());
        metadata = tool_metadata();
        let mut second = metadata["packages"][0].clone();
        second["id"] = json!("second-tool");
        second["version"] = json!("0.5.0");
        metadata["packages"].as_array_mut().unwrap().push(second);
        metadata["resolve"]["nodes"][0]["deps"]
            .as_array_mut()
            .unwrap()
            .push(json!({"pkg": "second-tool"}));
        assert!(locked_tool_version(&metadata).is_err());
    }

    #[test]
    fn ci_policy_preserves_scoring_but_removes_gate_and_display_controls() {
        let config = effective_config("try-weight = 0.5\ntop = 1\nmin = 20\nfail-above = true\nfail-regression = true\nmissing = 'skip'\nexclude = ['src/generated/**']\n[duplicates]\nenabled = true\n",30.0,0.01).unwrap().parse::<DocumentMut>().unwrap();
        assert_eq!(config["try-weight"].as_float(), Some(0.5));
        assert!(config.get("exclude").is_some());
        assert!(config.get("top").is_none());
        assert!(config.get("min").is_none());
        assert!(config.get("duplicates").is_none());
        assert_eq!(config["fail-above"].as_bool(), Some(false));
        assert_eq!(config["fail-regression"].as_bool(), Some(false));
        assert_eq!(config["missing"].as_str(), Some("pessimistic"));
    }
    #[test]
    fn improvements_cannot_cancel_regressions_and_new_threshold_is_strict() {
        let delta = json!({"entries":[{"status":"regressed","crap":3.0,"delta":0.02},{"status":"improved","crap":1.0},{"status":"new","crap":30.0},{"status":"new","crap":30.01},{"status":"unchanged","crap":100.0}],"removed":[]});
        let result = verdict(&delta, 30.0, 0.01).unwrap();
        assert_eq!(result["quality"], "fail");
        assert_eq!(result["regressed"], 1);
        assert_eq!(result["new_above"], 1);
        assert_eq!(result["existing_debt"], 1);
    }
    #[test]
    fn accepted_debt_and_moves_pass() {
        assert_eq!(verdict(&json!({"entries":[{"status":"unchanged","crap":100.0},{"status":"moved","crap":50.0}],"removed":[]}),30.0,0.01).unwrap()["quality"],"pass");
    }
    #[test]
    fn normalization_is_stable_and_rejects_escape() {
        let mut report = json!({"entries":[{"file":"/checkout/src/z.rs","function":"z","line":1},{"file":"/checkout/src/a.rs","function":"a","line":2}]});
        normalize(&mut report, Path::new("/checkout")).unwrap();
        assert_eq!(report["entries"][0]["file"], "src/a.rs");
        assert!(report.get("removed").is_none());
        assert!(
            normalize(
                &mut json!({"entries":[{"file":"/etc/passwd"}]}),
                Path::new("/checkout")
            )
            .is_err()
        );
        assert!(safe_relative(Path::new("../outside")).is_err());
    }

    #[test]
    fn smoke_requires_nonempty_matching_supported_report() {
        let valid = json!({"version":"0.6.1","entries":[{"file":"src/lib.rs","function":"choose","line":1,"cyclomatic":2.0,"coverage":100.0,"crap":2.0}],"diagnostics":{"analyzed_files":1,"lcov_files":1,"matched_files":1,"source_only":{"count":0,"examples":[]},"lcov_only":{"count":0,"examples":[]}}});
        validate(&valid, false).unwrap();
        let mut no_overlap = valid.clone();
        no_overlap["diagnostics"]["matched_files"] = json!(0);
        assert!(validate(&no_overlap, false).is_err());
        let mut unsupported = valid;
        unsupported["version"] = json!("0.7.0");
        assert!(validate(&unsupported, false).is_err());
    }

    #[test]
    fn selected_cargo_config_replaces_historical_settings_and_absence() {
        let fixture = tempfile::tempdir().unwrap();
        let project = fixture.path();
        fs::create_dir(project.join(".cargo")).unwrap();
        fs::write(
            project.join(".cargo/config.toml"),
            "[build]\ntarget = 'old'\n",
        )
        .unwrap();
        let selected = BTreeMap::from([(
            PathBuf::from(".cargo/config.toml"),
            "[build]\nrustflags = ['--cfg', 'selected']\n".into(),
        )]);
        apply_cargo_configs(project, Path::new(""), &selected).unwrap();
        assert_eq!(
            fs::read_to_string(project.join(".cargo/config.toml")).unwrap(),
            selected[Path::new(".cargo/config.toml")]
        );
        apply_cargo_configs(project, Path::new(""), &BTreeMap::new()).unwrap();
        assert!(!project.join(".cargo/config.toml").exists());
    }

    #[test]
    fn configuration_search_selects_nearest_scoring_and_all_cargo_ancestors() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let project = root.join("nested/crate");
        fs::create_dir_all(project.join(".cargo")).unwrap();
        fs::create_dir_all(root.join(".cargo")).unwrap();
        assert_eq!(config_from(&project, &root).unwrap(), "");
        assert!(cargo_configs(&project, &root).unwrap().is_empty());
        fs::write(root.join(".cargo-crap.toml"), "try-weight = 0.5\n").unwrap();
        fs::write(project.join(".cargo-crap.toml"), "try-weight = 1.0\n").unwrap();
        fs::write(root.join(".cargo/config"), "root config").unwrap();
        fs::write(project.join(".cargo/config.toml"), "project config").unwrap();
        assert_eq!(config_from(&project, &root).unwrap(), "try-weight = 1.0\n");
        let configs = cargo_configs(&project, &root).unwrap();
        assert_eq!(configs.len(), 2);
        assert_eq!(configs[Path::new(".cargo/config")], "root config");
        assert_eq!(
            configs[Path::new("nested/crate/.cargo/config.toml")],
            "project config"
        );
        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::remove_file(project.join(".cargo-crap.toml")).unwrap();
        std::os::unix::fs::symlink(outside.path(), project.join(".cargo-crap.toml")).unwrap();
        assert!(config_from(&project, &root).is_err());
        fs::remove_file(project.join(".cargo/config.toml")).unwrap();
        std::os::unix::fs::symlink(outside.path(), project.join(".cargo/config.toml")).unwrap();
        assert!(cargo_configs(&project, &root).is_err());
    }
}
