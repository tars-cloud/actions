use anyhow::{Context, Result, ensure};
use semver::Version;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use super::{numeric, state::Repository};
use crate::{github::Github, run};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    version: String,
    tag: String,
    commit_sha: String,
    assets: Vec<Files>,
    reference_artifacts: Vec<ReferenceArtifact>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Files {
    artifact: String,
    files: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceArtifact {
    artifact: String,
    file: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct References {
    schema: u32,
    version: String,
    commit_sha: String,
    references: Vec<Reference>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    name: String,
    url: String,
    #[serde(default)]
    reference: Option<String>,
    #[serde(default)]
    digest: Option<String>,
}

struct Asset {
    path: PathBuf,
    digest: String,
}

pub(super) struct Bundle {
    _directory: tempfile::TempDir,
    files: BTreeMap<String, Asset>,
    pub notes: String,
}

fn name(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 255
            && value != "."
            && value != ".."
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c)),
        "artifact and file names must contain only letters, digits, hyphens, underscores and dots"
    );
    Ok(())
}

fn regular_file(root: &Path, relative: &str) -> Result<PathBuf> {
    ensure!(!relative.is_empty(), "empty artifact path");
    let mut path = root.to_owned();
    for component in Path::new(relative).components() {
        let Component::Normal(part) = component else {
            anyhow::bail!("artifact file paths must be relative without traversal")
        };
        name(part.to_str().context("UTF-8 artifact path")?)?;
        path.push(part);
        ensure!(
            !fs::symlink_metadata(&path)?.file_type().is_symlink(),
            "artifact symlinks are not allowed"
        );
    }
    let metadata = fs::metadata(&path)?;
    ensure!(
        metadata.is_file() && metadata.len() > 0,
        "artifact file {} is missing or empty",
        path.display()
    );
    Ok(path)
}

fn digest(path: &Path) -> Result<String> {
    let output = run(Command::new("sha256sum").arg("--").arg(path))?;
    let digest = output
        .split_whitespace()
        .next()
        .context("SHA256 checksum")?;
    ensure!(
        digest.len() == 64 && digest.bytes().all(|c| c.is_ascii_hexdigit()),
        "invalid SHA256 checksum"
    );
    Ok(format!("sha256:{}", digest.to_ascii_lowercase()))
}

impl Bundle {
    pub fn download(
        repository: &Repository,
        source_run: &str,
        manifest_name: &str,
        commit: &str,
        version: &Version,
    ) -> Result<Self> {
        numeric(source_run)?;
        name(manifest_name)?;
        let source: Value = repository
            .github
            .get(&format!("actions/runs/{source_run}"))?;
        ensure!(
            source["head_sha"] == commit
                && source["head_branch"] == repository.branch
                && source["head_repository"]["full_name"] == repository.github.repo
                && (source["event"] == "push" || source["event"] == "workflow_dispatch"),
            "artifacts must come from this repository's exact default-branch release commit"
        );
        if std::env::var("GITHUB_RUN_ID").as_deref() != Ok(source_run) {
            ensure!(
                source["status"] == "completed" && source["conclusion"] == "success",
                "a different artifact run must have completed successfully"
            );
        }
        let mut available = BTreeSet::new();
        for page in 1.. {
            let result: Value = repository.github.get(&format!(
                "actions/runs/{source_run}/artifacts?per_page=100&page={page}"
            ))?;
            let artifacts = result["artifacts"].as_array().context("run artifacts")?;
            for artifact in artifacts
                .iter()
                .filter(|artifact| artifact["expired"] == false)
            {
                ensure!(
                    available.insert(
                        artifact["name"]
                            .as_str()
                            .context("artifact name")?
                            .to_owned()
                    ),
                    "duplicate artifact names in source run"
                );
            }
            if artifacts.len() < 100 {
                break;
            }
        }
        let directory = tempfile::tempdir()?;
        let mut downloaded = BTreeSet::new();
        let mut download = |artifact: &str| -> Result<PathBuf> {
            name(artifact)?;
            ensure!(
                available.contains(artifact),
                "required artifact {artifact} is missing or expired"
            );
            let path = directory.path().join(artifact);
            if downloaded.insert(artifact.to_owned()) {
                run(Command::new("gh")
                    .args([
                        "run",
                        "download",
                        source_run,
                        "--repo",
                        &repository.github.repo,
                        "--name",
                        artifact,
                        "--dir",
                    ])
                    .arg(&path))?;
            }
            Ok(path)
        };
        let root = download(manifest_name)?;
        let manifest: Manifest =
            serde_json::from_slice(&fs::read(regular_file(&root, "release-manifest.json")?)?)?;
        ensure!(
            manifest.schema == 1
                && manifest.version == version.to_string()
                && manifest.tag == format!("v{version}")
                && manifest.commit_sha == commit,
            "release manifest identity differs from the approved candidate"
        );
        let mut files = BTreeMap::new();
        for group in manifest.assets {
            ensure!(
                !group.files.is_empty(),
                "asset groups must list their expected files"
            );
            let root = download(&group.artifact)?;
            for file in group.files {
                let path = regular_file(&root, &file)?;
                let filename = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .context("asset filename")?
                    .to_owned();
                let asset = Asset {
                    digest: digest(&path)?,
                    path,
                };
                ensure!(
                    files.insert(filename.clone(), asset).is_none(),
                    "duplicate release asset name {filename}"
                );
            }
        }
        verify_checksums(&files)?;
        let mut notes = String::new();
        for group in manifest.reference_artifacts {
            let root = download(&group.artifact)?;
            let references: References =
                serde_json::from_slice(&fs::read(regular_file(&root, &group.file)?)?)?;
            ensure!(
                references.schema == 1
                    && references.version == version.to_string()
                    && references.commit_sha == commit,
                "external references do not belong to the release candidate"
            );
            ensure!(
                !references.references.is_empty(),
                "reference artifacts must contain the expected published references"
            );
            for reference in references.references {
                ensure!(
                    !reference.name.is_empty() && !reference.name.chars().any(char::is_control),
                    "invalid reference name"
                );
                ensure!(
                    reference.url.starts_with("https://")
                        && !reference
                            .url
                            .chars()
                            .any(|c| c.is_whitespace() || "<>\\".contains(c)),
                    "external links must be HTTPS URLs without whitespace"
                );
                let label = reference
                    .name
                    .replace('\\', "\\\\")
                    .replace('[', "\\[")
                    .replace(']', "\\]");
                notes.push_str(&format!("\n- [{label}](<{}>)", reference.url));
                if let Some(reference) = reference.reference {
                    ensure!(
                        !reference.is_empty()
                            && reference
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b"/.:@_-".contains(&b)),
                        "invalid external artifact reference"
                    );
                    notes.push_str(&format!(": `{reference}`"));
                }
                if let Some(digest) = reference.digest {
                    let checksum = digest
                        .strip_prefix("sha256:")
                        .context("reference digest must use sha256")?;
                    ensure!(
                        checksum.len() == 64 && checksum.bytes().all(|c| c.is_ascii_hexdigit()),
                        "invalid reference digest"
                    );
                    notes.push_str(&format!(" (`{digest}`)"));
                }
            }
        }
        if !notes.is_empty() {
            notes = format!("\n\n### Published Artifacts\n{notes}\n");
        }
        Ok(Self {
            _directory: directory,
            files,
            notes,
        })
    }

    pub fn attach(&self, github: &Github, id: u64, tag: &str, draft: bool) -> Result<()> {
        let existing: Vec<Value> = github.list(&format!("releases/{id}/assets?per_page=100"))?;
        ensure!(
            existing.iter().all(|asset| asset["name"]
                .as_str()
                .is_some_and(|name| self.files.contains_key(name))),
            "release contains assets not declared by the manifest"
        );
        for (name, asset) in &self.files {
            let old = existing.iter().find(|file| file["name"] == *name);
            let matches = if let Some(old) = old {
                matches_asset(github, tag, name, old, asset)?
            } else {
                false
            };
            if matches {
                continue;
            }
            ensure!(
                draft,
                "published release asset {name} is missing or differs; published releases are never modified"
            );
            run(Command::new("gh")
                .args([
                    "release",
                    "upload",
                    tag,
                    "--repo",
                    &github.repo,
                    "--clobber",
                ])
                .arg(&asset.path))?;
        }
        let uploaded: Vec<Value> = github.list(&format!("releases/{id}/assets?per_page=100"))?;
        ensure!(
            uploaded.len() == self.files.len()
                && uploaded.iter().all(|asset| asset["state"] == "uploaded"
                    && asset["name"]
                        .as_str()
                        .is_some_and(|name| self.files.contains_key(name))),
            "release asset upload is incomplete"
        );
        for old in uploaded {
            let name = old["name"].as_str().context("release asset name")?;
            ensure!(
                matches_asset(github, tag, name, &old, &self.files[name])?,
                "uploaded release asset {name} has the wrong digest"
            );
        }
        Ok(())
    }
}

fn matches_asset(
    github: &Github,
    tag: &str,
    name: &str,
    old: &Value,
    asset: &Asset,
) -> Result<bool> {
    // GitHub can leave an empty starter asset after a failed upload.
    if old["state"] != "uploaded" {
        return Ok(false);
    }
    if let Some(old_digest) = old["digest"].as_str() {
        return Ok(old_digest == asset.digest);
    }
    let directory = tempfile::tempdir()?;
    run(Command::new("gh")
        .args([
            "release",
            "download",
            tag,
            "--repo",
            &github.repo,
            "--pattern",
            name,
            "--dir",
        ])
        .arg(directory.path()))?;
    Ok(digest(&regular_file(directory.path(), name)?)? == asset.digest)
}

fn verify_checksums(files: &BTreeMap<String, Asset>) -> Result<()> {
    for (name, checksum) in files
        .iter()
        .filter(|(name, _)| name.ends_with(".sha256") || name.as_str() == "SHA256SUMS")
    {
        let contents = fs::read_to_string(&checksum.path)
            .with_context(|| format!("read checksum file {name}"))?;
        ensure!(!contents.trim().is_empty(), "empty checksum file {name}");
        for line in contents.lines().filter(|line| !line.trim().is_empty()) {
            let (expected, file) = line
                .split_once(char::is_whitespace)
                .context("checksum must contain SHA256 and filename")?;
            let file = file.trim_start().trim_start_matches('*');
            let asset = files
                .get(file)
                .with_context(|| format!("checksum references undeclared file {file}"))?;
            ensure!(
                asset.digest == format!("sha256:{}", expected.to_ascii_lowercase()),
                "checksum mismatch for {file}"
            );
        }
    }
    Ok(())
}
