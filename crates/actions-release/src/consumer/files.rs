use anyhow::{Context, Result, ensure};
use semver::Version;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use toml_edit::{DocumentMut, Item, value};

use super::git;
use crate::{project, run};

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
    workspace_root: PathBuf,
}

#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    version: String,
    manifest_path: PathBuf,
}

pub(super) struct Project {
    pub version: Version,
    pub manifests: Vec<String>,
    packages: BTreeMap<String, Version>,
}

fn stable(version: &str) -> Result<Version> {
    let version = Version::parse(version)?;
    ensure!(
        version.pre.is_empty() && version.build.is_empty(),
        "only stable Rust releases are supported"
    );
    Ok(version)
}

pub(super) fn version(manifest: &str) -> Result<Version> {
    let doc: DocumentMut = manifest.parse()?;
    stable(
        doc.get("workspace")
            .and_then(|w| w.get("package"))
            .and_then(|p| p.get("version"))
            .or_else(|| doc.get("package").and_then(|p| p.get("version")))
            .and_then(Item::as_str)
            .context("root Cargo.toml must own package.version or workspace.package.version")?,
    )
}

impl Project {
    pub fn read() -> Result<Self> {
        let version = version(&fs::read_to_string("Cargo.toml")?)?;
        let metadata: Metadata = serde_json::from_str(&run(Command::new("cargo").args([
            "metadata",
            "--no-deps",
            "--locked",
            "--format-version",
            "1",
        ]))?)?;
        let root = fs::canonicalize(".")?;
        ensure!(
            fs::canonicalize(metadata.workspace_root)? == root,
            "release environment must be the Cargo workspace root"
        );
        let mut manifests = vec!["Cargo.toml".to_owned()];
        let mut packages = BTreeMap::new();
        for package in metadata.packages {
            if !metadata.workspace_members.contains(&package.id) {
                continue;
            }
            ensure!(
                package.version == version.to_string(),
                "workspace package {} must use the root version {version}",
                package.name
            );
            let manifest = fs::canonicalize(&package.manifest_path)?;
            let relative = manifest
                .strip_prefix(&root)
                .context("workspace manifests must remain inside the repository")?
                .to_str()
                .context("UTF-8 manifest path")?
                .to_owned();
            if relative != "Cargo.toml" {
                let doc: DocumentMut = fs::read_to_string(&manifest)?.parse()?;
                ensure!(
                    inherits_version(&doc),
                    "{} must inherit version.workspace = true",
                    relative
                );
                manifests.push(relative);
            }
            packages.insert(package.name, stable(&package.version)?);
        }
        ensure!(!packages.is_empty(), "Cargo workspace contains no packages");
        manifests.sort();
        manifests.dedup();
        ensure!(
            Path::new("Cargo.lock").is_file(),
            "commit Cargo.lock before using Rust release automation"
        );
        Ok(Self {
            version,
            manifests,
            packages,
        })
    }

    pub fn expected(&self, base: &str, next: &Version) -> Result<BTreeMap<String, String>> {
        let mut files = BTreeMap::new();
        let previous = version(&project::file_at(base, "Cargo.toml")?)?;
        for path in &self.manifests {
            let mut doc: DocumentMut = project::file_at(base, path)?.parse()?;
            if path == "Cargo.toml" {
                if doc
                    .get("workspace")
                    .and_then(|w| w.get("package"))
                    .and_then(|p| p.get("version"))
                    .is_some()
                {
                    doc["workspace"]["package"]["version"] = value(next.to_string());
                    if doc.get("package").is_some() {
                        ensure!(
                            inherits_version(&doc),
                            "root package must inherit the workspace version"
                        );
                    }
                } else {
                    doc["package"]["version"] = value(next.to_string());
                }
            }
            update_dependencies(doc.as_item_mut(), &self.packages, next)?;
            files.insert(path.clone(), doc.to_string());
        }
        let mut lock: DocumentMut = project::file_at(base, "Cargo.lock")?.parse()?;
        update_lock(&mut lock, &self.packages, &previous, next)?;
        files.insert("Cargo.lock".to_owned(), lock.to_string());
        Ok(files)
    }

    pub fn write(&self, base: &str, next: &Version) -> Result<String> {
        for (path, contents) in self.expected(base, next)? {
            fs::write(path, contents)?;
        }
        run(Command::new("cargo").args([
            "metadata",
            "--no-deps",
            "--locked",
            "--format-version",
            "1",
        ]))?;
        let changelog =
            run(convco("changelog")?.args(["--unreleased", &next.to_string(), "HEAD"]))?;
        ensure!(
            changelog.lines().find_map(project::heading_version) == Some(next.clone()),
            "Convco changelog must begin with the release version"
        );
        fs::write(
            "CHANGELOG.md",
            format!("<!-- markdownlint-disable MD024 -->\n{changelog}\n"),
        )?;
        Ok(changelog)
    }
}

fn update_lock(
    lock: &mut DocumentMut,
    workspace: &BTreeMap<String, Version>,
    previous: &Version,
    next: &Version,
) -> Result<()> {
    let previous = previous.to_string();
    let packages = lock["package"]
        .as_array_of_tables_mut()
        .context("Cargo.lock packages")?;
    for package in packages.iter_mut() {
        if !package.contains_key("source")
            && package["version"].as_str() == Some(previous.as_str())
            && package["name"]
                .as_str()
                .is_some_and(|name| workspace.contains_key(name))
        {
            package["version"] = value(next.to_string());
        }
        if let Some(dependencies) = package.get_mut("dependencies").and_then(Item::as_array_mut) {
            for dependency in dependencies.iter_mut() {
                let Some(text) = dependency.as_str() else {
                    continue;
                };
                let words: Vec<_> = text.split(' ').collect();
                if words.len() == 2 && workspace.contains_key(words[0]) && words[1] == previous {
                    *dependency = toml_edit::Value::from(format!("{} {next}", words[0]));
                }
            }
        }
    }
    Ok(())
}

fn update_dependencies(
    item: &mut Item,
    packages: &BTreeMap<String, Version>,
    next: &Version,
) -> Result<()> {
    let Some(table) = item.as_table_like_mut() else {
        return Ok(());
    };
    for key in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(dependencies) = table.get_mut(key).and_then(Item::as_table_like_mut) {
            for (name, dependency) in dependencies.iter_mut() {
                let Some(dependency) = dependency.as_table_like_mut() else {
                    continue;
                };
                let actual = dependency
                    .get("package")
                    .and_then(Item::as_str)
                    .unwrap_or(name.get());
                if dependency.contains_key("path")
                    && packages.contains_key(actual)
                    && let Some(requirement) = dependency.get("version").and_then(Item::as_str)
                {
                    let prefix = if requirement.starts_with('=') {
                        "="
                    } else if requirement.starts_with('~') {
                        "~"
                    } else {
                        ""
                    };
                    dependency.insert("version", value(format!("{prefix}{next}")));
                }
            }
        }
    }
    if let Some(workspace) = table.get_mut("workspace") {
        update_dependencies(workspace, packages, next)?;
    }
    if let Some(targets) = table.get_mut("target").and_then(Item::as_table_like_mut) {
        for (_, target) in targets.iter_mut() {
            update_dependencies(target, packages, next)?;
        }
    }
    Ok(())
}

fn inherits_version(doc: &DocumentMut) -> bool {
    doc.get("package")
        .and_then(|p| p.get("version"))
        .and_then(|v| v.get("workspace"))
        .and_then(Item::as_bool)
        == Some(true)
}

fn convco_command(operation: &str) -> Command {
    let mut command = Command::new("convco");
    // Ambient overrides must not silently change the reviewed repository policy.
    for (key, _) in std::env::vars().filter(|(key, _)| key.starts_with("CONVCO_")) {
        command.env_remove(key);
    }
    command.arg(operation);
    command
}

fn convco(operation: &str) -> Result<Command> {
    let help = run(convco_command(operation).arg("--help"))?;
    let mut command = convco_command(operation);
    command.args(["--prefix", "v"]);
    // Older nixpkgs Convco supports only SemVer and has no scheme selector.
    if help
        .split_whitespace()
        .any(|word| word == "--version-scheme")
    {
        command.args(["--version-scheme", "semver"]);
    }
    let configs: Vec<_> = [".convco", ".versionrc"]
        .into_iter()
        .filter(|path| Path::new(path).is_file())
        .collect();
    ensure!(
        configs.len() <= 1,
        "keep only one Convco config: .convco or .versionrc"
    );
    if let Some(path) = configs.first() {
        let config: serde_norway::Value = serde_norway::from_str(&fs::read_to_string(path)?)?;
        ensure!(
            config.is_mapping(),
            "Convco configuration must be a mapping"
        );
        command.args(["--config", path]);
    }
    Ok(command)
}

pub(super) fn next_version(base: &str, initial: &Version) -> Result<Version> {
    stable(&run(convco("version")?.args([
        "--bump",
        "--initial-bump-version",
        &initial.to_string(),
        base,
    ]))?)
}

pub(super) fn verify(commit: &str, head: &str) -> Result<Version> {
    let base = git(&["rev-parse", &format!("{head}^")])?;
    ensure!(
        base == git(&["rev-parse", &format!("{commit}^1")])?,
        "stale release PR: refresh it against the default branch before merging"
    );
    ensure!(
        git(&["rev-parse", &format!("{head}^{{tree}}")])?
            == git(&["rev-parse", &format!("{commit}^{{tree}}")])?,
        "merged release differs from the reviewed release PR"
    );
    let project = Project::read()?;
    let expected = project.expected(&base, &project.version)?;
    let changed = git(&["diff", "--name-only", &base, commit])?;
    ensure!(
        !changed.is_empty()
            && changed
                .lines()
                .all(|path| path == "CHANGELOG.md" || expected.contains_key(path)),
        "release PR may change only version manifests, Cargo.lock and CHANGELOG.md"
    );
    for (path, contents) in expected {
        ensure!(
            project::file_at(commit, &path)?.trim_end() == contents.trim_end(),
            "release PR changed {path} beyond derived version metadata"
        );
    }
    let initial = version(&project::file_at(&base, "Cargo.toml")?)?;
    ensure!(
        next_version(&base, &initial)? == project.version,
        "approved Cargo version differs from Convco's release calculation"
    );
    let changelog = project::file_at(commit, "CHANGELOG.md")?;
    ensure!(
        changelog.lines().find_map(project::heading_version) == Some(project.version.clone()),
        "changelog must begin with the release version"
    );
    Ok(project.version)
}

pub(super) fn notes(changelog: &str, version: &Version) -> Result<String> {
    let lines: Vec<_> = changelog.lines().collect();
    let start = lines
        .iter()
        .position(|line| project::heading_version(line).as_ref() == Some(version))
        .context("approved changelog section")?;
    Ok(lines[start + 1..]
        .iter()
        .take_while(|line| project::heading_version(line).is_none())
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lockfile_bump_preserves_registry_packages_with_the_same_name() {
        let mut lock: DocumentMut = r#"
version = 4
[[package]]
name = "app"
version = "1.2.3"
dependencies = ["core 1.2.3", "core 0.9.0", "core 1.2.3 (registry+https://example.invalid/index)"]
[[package]]
name = "core"
version = "1.2.3"
[[package]]
name = "core"
version = "0.9.0"
source = "registry+https://example.invalid/index"
[[package]]
name = "core"
version = "1.2.3"
source = "registry+https://example.invalid/index"
"#
        .parse()
        .unwrap();
        let previous = Version::new(1, 2, 3);
        let workspace = BTreeMap::from([
            ("app".into(), previous.clone()),
            ("core".into(), previous.clone()),
        ]);
        update_lock(&mut lock, &workspace, &previous, &Version::new(1, 3, 0)).unwrap();
        let packages = lock["package"].as_array_of_tables().unwrap();
        assert_eq!(packages.get(0).unwrap()["version"].as_str(), Some("1.3.0"));
        assert_eq!(packages.get(1).unwrap()["version"].as_str(), Some("1.3.0"));
        assert_eq!(packages.get(2).unwrap()["version"].as_str(), Some("0.9.0"));
        assert_eq!(packages.get(3).unwrap()["version"].as_str(), Some("1.2.3"));
        let dependencies = packages.get(0).unwrap()["dependencies"].as_array().unwrap();
        assert_eq!(dependencies.get(0).unwrap().as_str(), Some("core 1.3.0"));
        assert_eq!(dependencies.get(1).unwrap().as_str(), Some("core 0.9.0"));
        assert_eq!(
            dependencies.get(2).unwrap().as_str(),
            Some("core 1.2.3 (registry+https://example.invalid/index)")
        );
    }
}
