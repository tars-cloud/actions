use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub version: u32,
    #[serde(default)]
    pub sources: Vec<String>,
    pub tests: Vec<Case>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(crate) struct Case {
    pub id: String,
    pub description: String,
    pub command: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub command_paths: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    #[serde(default)]
    pub commands: BTreeMap<String, Vec<Response>>,
    #[serde(default = "timeout")]
    pub timeout_seconds: u64,
    pub expect: Expect,
}

fn timeout() -> u64 {
    30
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(crate) struct Response {
    pub args: Vec<String>,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
    pub exit: u8,
    #[serde(default)]
    pub forward: Option<usize>,
    #[serde(default)]
    pub prepend_path: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(crate) struct Expect {
    pub exit: i32,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub github_output: Option<BTreeMap<String, String>>,
    pub github_env: Option<BTreeMap<String, String>>,
    pub github_path: Option<Vec<String>>,
    #[serde(default)]
    pub files: BTreeMap<String, Option<String>>,
    pub calls: Vec<Call>,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Call {
    pub command: String,
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

pub(crate) struct Suite {
    pub action: String,
    pub manifest: Manifest,
}

pub(crate) fn relative(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty()
            && Path::new(path)
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
        "expected a relative path without traversal: {path:?}"
    );
    Ok(())
}

pub(crate) fn load(path: &Path) -> Result<Manifest> {
    let value: serde_json::Value = serde_norway::from_str(&fs::read_to_string(path)?)
        .with_context(|| format!("parse {}", path.display()))?;
    let schema = serde_json::from_str(include_str!("../../../schemas/test.schema.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    let errors: Vec<_> = validator
        .iter_errors(&value)
        .map(|e| format!("{}: {e}", e.instance_path()))
        .collect();
    ensure!(
        errors.is_empty(),
        "{}: {}",
        path.display(),
        errors.join("; ")
    );
    let manifest: Manifest = serde_json::from_value(value)?;
    ensure!(manifest.version == 1, "unsupported manifest version");
    let mut ids = BTreeSet::new();
    for source in &manifest.sources {
        relative(source)?;
    }
    for case in &manifest.tests {
        ensure!(ids.insert(&case.id), "duplicate case id: {}", case.id);
        for path in case.files.keys().chain(case.expect.files.keys()) {
            relative(path)?;
        }
        for name in case.env.keys() {
            ensure!(
                !matches!(
                    name.as_str(),
                    "PATH"
                        | "HOME"
                        | "TMPDIR"
                        | "GITHUB_WORKSPACE"
                        | "GITHUB_OUTPUT"
                        | "GITHUB_ENV"
                        | "GITHUB_PATH"
                        | "BASH_ENV"
                        | "ENV"
                        | "SHELLOPTS"
                        | "BASHOPTS"
                ) && !name.starts_with("TACT_"),
                "reserved environment variable: {name}"
            );
        }
        for (name, responses) in &case.commands {
            ensure!(
                !matches!(name.as_str(), "bash" | "dirname"),
                "reserved command: {name}"
            );
            let mut args = BTreeSet::new();
            for response in responses {
                ensure!(
                    args.insert(&response.args),
                    "duplicate response arguments for {name}"
                );
            }
        }
        for tool in &case.tools {
            ensure!(
                !case.commands.contains_key(tool),
                "tool and mock conflict: {tool}"
            );
        }
        for name in case.command_paths.keys() {
            ensure!(
                case.commands.contains_key(name),
                "command-paths has no mock: {name}"
            );
        }
    }
    Ok(manifest)
}

pub(crate) fn discover(root: &Path, selected: Option<&str>) -> Result<Vec<Suite>> {
    if let Some(name) = selected {
        relative(name)?;
        ensure!(
            root.join(name).join("test.yaml").is_file(),
            "action {name} has no test.yaml"
        );
    }
    let mut suites = Vec::new();
    let actions = root.join("composite");
    let mut directories = vec![actions.clone()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let action = entry
                .path()
                .strip_prefix(root)?
                .to_string_lossy()
                .into_owned();
            let path = entry.path().join("test.yaml");
            let has_action = entry.path().join("action.yml").is_file()
                || entry.path().join("action.yaml").is_file();
            if !path.is_file() && !has_action {
                // Private helpers may be nested below an action's scripts directory.
                if directory != actions {
                    directories.push(entry.path());
                }
                continue;
            }
            let scripts = entry.path().join("scripts");
            if scripts.is_dir() && !scripts.is_symlink() {
                directories.push(scripts);
            }
            if selected.is_some_and(|name| {
                name != action && !action.starts_with(&format!("{name}/scripts/"))
            }) {
                continue;
            }
            ensure!(path.is_file(), "action {action} has no test.yaml");
            ensure!(
                entry.path().join("action.yml").is_file()
                    || entry.path().join("action.yaml").is_file(),
                "{action}/test.yaml has no action metadata"
            );
            suites.push(Suite {
                action,
                manifest: load(&path)?,
            });
        }
    }
    suites.sort_by(|a, b| a.action.cmp(&b.action));
    ensure!(!suites.is_empty(), "no action test.yaml manifests found");
    Ok(suites)
}
