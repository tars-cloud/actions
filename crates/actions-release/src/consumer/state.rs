use anyhow::{Context, Result, ensure};
use semver::Version;
use serde_json::{Value, json};
use std::fs;

use super::git;
use super::{artifacts, files, skipped, text};
use crate::{github::Github, project, report};

const MARKER: &str = "<!-- tars-cloud/actions:release-rust:v1 -->";
const RELEASE_BRANCH: &str = "release/next";
const REPLACEMENT_PREFIX: &str = "<!-- tars-cloud/actions:release-rust:replacement:v1:";

pub(super) struct Repository {
    pub github: Github,
    pub branch: String,
    tip: String,
    pub(super) trigger: String,
    pub(super) recovery: bool,
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

impl Repository {
    pub fn open(commit: &str, recovery: bool) -> Result<Self> {
        ensure!(
            std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true"),
            "shared releases must run in GitHub Actions"
        );
        let github = Github::from_env()?;
        let metadata: Value = github.get("")?;
        let branch = text(&metadata, "default_branch")?.to_owned();
        git(&["check-ref-format", &format!("refs/heads/{branch}")])?;
        ensure!(
            std::env::var("GITHUB_REF").as_deref() == Ok(&format!("refs/heads/{branch}")),
            "release workflows must run from the default branch"
        );
        let event = std::env::var("GITHUB_EVENT_NAME")?;
        ensure!(
            event == "push" || event == "workflow_dispatch",
            "release workflows require push or workflow_dispatch"
        );
        let trigger = std::env::var("GITHUB_SHA")?;
        project::full_sha(&trigger)?;
        if recovery {
            ensure!(
                event == "workflow_dispatch",
                "release recovery requires a manual default-branch dispatch"
            );
        } else {
            ensure!(
                trigger == commit,
                "release commit must match the triggering run"
            );
        }
        ensure!(
            fs::canonicalize(".")? == fs::canonicalize(git(&["rev-parse", "--show-toplevel"])?)?,
            "release environment must be the repository root"
        );
        ensure!(
            git(&["rev-parse", "HEAD"])? == commit,
            "checkout must match the exact release input commit"
        );
        ensure!(
            git(&["status", "--porcelain", "--untracked-files=no"])?.is_empty(),
            "tracked release checkout must be clean"
        );
        git(&[
            "fetch",
            "--force",
            "origin",
            &format!("+refs/heads/{branch}:refs/remotes/origin/{branch}"),
            "+refs/tags/*:refs/tags/*",
        ])?;
        let tip = git(&["rev-parse", &format!("refs/remotes/origin/{branch}")])?;
        git(&["merge-base", "--is-ancestor", commit, &tip])?;
        git(&["merge-base", "--is-ancestor", commit, &trigger])?;
        git(&["merge-base", "--is-ancestor", &trigger, &tip])?;
        let repository = Self {
            github,
            branch,
            tip,
            trigger,
            recovery,
        };
        if recovery {
            ensure!(
                repository.pull(commit)?.is_some(),
                "recovery requires a merged managed release PR"
            );
            ensure!(
                repository.published(commit)? || repository.pending()?.as_deref() == Some(commit),
                "recovery must select the pending managed release"
            );
        }
        Ok(repository)
    }

    fn managed(&self, pr: &Value) -> bool {
        pr["head"]["ref"] == RELEASE_BRANCH
            && pr["base"]["ref"] == self.branch
            && pr["head"]["repo"]["full_name"] == self.github.repo
            && pr["base"]["repo"]["full_name"] == self.github.repo
            && pr["body"]
                .as_str()
                .is_some_and(|body| body.lines().any(|line| line == MARKER))
    }

    fn pull(&self, commit: &str) -> Result<Option<Value>> {
        let pulls: Vec<Value> = self
            .github
            .list(&format!("commits/{commit}/pulls?per_page=100"))?;
        let mut matching: Vec<_> = pulls
            .into_iter()
            .filter(|pr| {
                !pr["merged_at"].is_null()
                    && pr["merge_commit_sha"] == commit
                    && pr["head"]["ref"] == RELEASE_BRANCH
            })
            .collect();
        ensure!(
            matching.len() <= 1,
            "multiple release PRs match this commit"
        );
        if let Some(pr) = matching.first() {
            ensure!(
                self.managed(pr),
                "release/next was not created by this shared release process"
            );
        }
        Ok(matching.pop())
    }

    pub fn tag_target(&self, tag: &str) -> Result<Option<String>> {
        if git(&["tag", "--list", tag])?.is_empty() {
            return Ok(None);
        }
        Ok(Some(git(&[
            "rev-parse",
            &format!("refs/tags/{tag}^{{commit}}"),
        ])?))
    }

    pub fn releases(&self) -> Result<Vec<Value>> {
        self.github.list("releases?per_page=100")
    }

    fn published(&self, commit: &str) -> Result<bool> {
        let version = files::version(&project::file_at(commit, "Cargo.toml")?)?;
        let tag = format!("v{version}");
        if let Some(target) = self.tag_target(&tag)? {
            ensure!(
                target == commit,
                "immutable version tag {tag} points to another commit"
            );
        }
        let released = self.releases()?.iter().any(|release| {
            release["tag_name"] == tag
                && release["draft"] == false
                && release["prerelease"] == false
        });
        ensure!(
            !released || self.tag_target(&tag)?.is_some(),
            "published release is missing its version tag"
        );
        Ok(released)
    }

    fn pending(&self) -> Result<Option<String>> {
        let owner = self
            .github
            .repo
            .split('/')
            .next()
            .context("repository owner")?;
        let mut prs: Vec<Value> = self.github.list(&format!(
            "pulls?state=closed&base={}&head={}&per_page=100",
            encode(&self.branch),
            encode(&format!("{owner}:{RELEASE_BRANCH}"))
        ))?;
        prs.sort_by(|a, b| b["merged_at"].as_str().cmp(&a["merged_at"].as_str()));
        if let Some(pr) = prs
            .iter()
            .find(|pr| self.managed(pr) && !pr["merged_at"].is_null())
        {
            let commit = text(pr, "merge_commit_sha")?;
            project::full_sha(commit)?;
            git(&["merge-base", "--is-ancestor", commit, &self.tip])?;
            if self.published(commit)? {
                return Ok(None);
            }
            return Ok(Some(commit.to_owned()));
        }
        Ok(None)
    }

    fn replacement_version(&self, commit: &str, replaced: &str) -> Result<Version> {
        ensure!(
            !self.recovery
                && std::env::var("GITHUB_EVENT_NAME").as_deref() == Ok("workflow_dispatch"),
            "candidate replacement requires a manual default-branch dispatch"
        );
        ensure!(
            commit == self.tip,
            "replacement source must be the current default-branch tip"
        );
        ensure!(
            self.pending()?.as_deref() == Some(replaced),
            "replacement must identify the exact pending managed release"
        );
        ensure!(
            commit != replaced,
            "replacement requires newer reviewed source"
        );
        git(&["merge-base", "--is-ancestor", replaced, commit])?;
        let version = files::version(&project::file_at(replaced, "Cargo.toml")?)?;
        let tag = format!("v{version}");
        ensure!(
            self.tag_target(&tag)?.is_none(),
            "tagged releases cannot be replaced"
        );
        // Draft visibility must match the identity that can prepare the replacement.
        ensure!(
            !self
                .github
                .list_with_write_token::<Value>("releases?per_page=100")?
                .iter()
                .any(|release| release["tag_name"] == tag),
            "existing releases or drafts must be recovered from their original source"
        );
        Ok(version)
    }

    pub fn candidate(&self, commit: &str, replacement: Option<&str>) -> Result<Value> {
        if let Some(replaced) = replacement {
            let version = self.replacement_version(commit, replaced)?;
            return Ok(
                json!({"prepare-ready":"true", "release-ready":"false", "commit-sha":commit, "version":version.to_string()}),
            );
        }
        if let Some(pr) = self.pull(commit)? {
            if self.published(commit)? {
                return Ok(skipped("Release already published."));
            }
            if self.pending()?.as_deref() != Some(commit) {
                return Ok(skipped("A newer managed release replaced this candidate."));
            }
            let version = self.verify(commit, &pr)?;
            return Ok(
                json!({"prepare-ready":"false", "release-ready":"true", "version":version.to_string(), "tag":format!("v{version}"), "commit-sha":commit, "pr-number":pr["number"]}),
            );
        }
        if let Some(pending) = self.pending()? {
            return Ok(skipped(&format!(
                "Release {pending} is awaiting publication; rerun its release workflow."
            )));
        }
        if commit != self.tip {
            return Ok(skipped(
                "Default branch advanced; the newer run will prepare the release.",
            ));
        }
        Ok(json!({"prepare-ready":"true", "release-ready":"false", "commit-sha":commit}))
    }

    fn verify(&self, commit: &str, pr: &Value) -> Result<Version> {
        let declared = files::version(&project::file_at(commit, "Cargo.toml")?)?;
        if let Some(target) = self.tag_target(&format!("v{declared}"))? {
            ensure!(
                target == commit,
                "immutable version tag points to another commit"
            );
        }
        let number = pr["number"].as_u64().context("release PR number")?;
        let head = text(&pr["head"], "sha")?;
        project::full_sha(head)?;
        git(&["fetch", "origin", &format!("refs/pull/{number}/head")])?;
        ensure!(
            git(&["rev-parse", "FETCH_HEAD"])? == head,
            "release PR head changed"
        );
        let replacement = self.approved_replacement_version(commit, pr)?;
        let version = files::verify(commit, head, replacement.as_ref())?;
        for tag in git(&["tag", "--list", "v*"])?.lines() {
            if let Ok(other) = Version::parse(tag.trim_start_matches('v')) {
                ensure!(other <= version, "a newer version {tag} already exists");
            }
        }
        Ok(version)
    }

    fn approved_replacement_version(&self, commit: &str, pr: &Value) -> Result<Option<Version>> {
        let body = text(pr, "body")?;
        let markers: Vec<_> = body
            .lines()
            .filter_map(|line| line.strip_prefix(REPLACEMENT_PREFIX))
            .collect();
        ensure!(
            markers.len() <= 1,
            "multiple replacement sources in release PR"
        );
        let Some(marker) = markers.first() else {
            return Ok(None);
        };
        let replaced = marker
            .strip_suffix(" -->")
            .context("malformed replacement source")?;
        project::full_sha(replaced)?;
        ensure!(replaced != commit, "release cannot replace itself");
        self.pull(replaced)?
            .context("replacement source must be a merged managed release PR")?;
        git(&[
            "merge-base",
            "--is-ancestor",
            replaced,
            &format!("{commit}^1"),
        ])?;
        Ok(Some(files::version(&project::file_at(
            replaced,
            "Cargo.toml",
        )?)?))
    }

    fn require_current(&self, commit: &str) -> Result<()> {
        ensure!(
            self.published(commit)? || self.pending()?.as_deref() == Some(commit),
            "publication requires the latest approved managed release"
        );
        Ok(())
    }

    fn open_pull(&self) -> Result<Option<Value>> {
        let owner = self
            .github
            .repo
            .split('/')
            .next()
            .context("repository owner")?;
        let mut prs: Vec<Value> = self.github.list(&format!(
            "pulls?state=open&base={}&head={}&per_page=100",
            encode(&self.branch),
            encode(&format!("{owner}:{RELEASE_BRANCH}"))
        ))?;
        ensure!(prs.len() <= 1, "multiple release PRs exist");
        for pr in &prs {
            ensure!(
                self.managed(pr),
                "existing release/next PR is not managed by this release process"
            );
        }
        Ok(prs.pop())
    }

    pub fn prepare(&self, commit: &str, replacement: Option<&str>) -> Result<Value> {
        if self.candidate(commit, replacement)?["prepare-ready"] != "true" {
            return Ok(skipped(
                "This commit is not eligible for release preparation.",
            ));
        }
        let project = files::Project::read()?;
        let version = if let Some(replaced) = replacement {
            self.replacement_version(commit, replaced)?
        } else {
            files::next_version(commit, &project.version)?
        };
        let tag = format!("v{version}");
        if self.tag_target(&tag)?.is_some() {
            return Ok(skipped(
                "No version bump: no releasable conventional commits.",
            ));
        }
        ensure!(
            replacement.is_some() || version >= project.version,
            "Convco calculated a version older than Cargo.toml"
        );
        let existing = self.open_pull()?;
        let previous = git(&[
            "ls-remote",
            "origin",
            &format!("refs/heads/{RELEASE_BRANCH}"),
        ])?;
        let previous_sha = previous.split_whitespace().next().unwrap_or("");
        if !previous_sha.is_empty() {
            git(&["fetch", "origin", &format!("refs/heads/{RELEASE_BRANCH}")])?;
        }
        git(&["checkout", "-B", RELEASE_BRANCH, commit])?;
        let changelog = project.write(commit, &version)?;
        let mut paths: Vec<&str> = vec!["add", "--", "Cargo.lock", "CHANGELOG.md"];
        paths.extend(project.manifests.iter().map(String::as_str));
        git(&paths)?;
        ensure!(
            !git(&["diff", "--cached", "--name-only"])?.is_empty(),
            "release version has no prepared changes; inspect unpublished tags and release state"
        );
        let unchanged = commit_prepared(commit, &tag, previous_sha)?;
        let remote = git(&[
            "ls-remote",
            "origin",
            &format!("refs/heads/{}", self.branch),
        ])?;
        if remote.split_whitespace().next() != Some(commit) {
            return Ok(skipped(
                "Default branch advanced during preparation; a newer run will refresh the PR.",
            ));
        }
        if !unchanged {
            git(&[
                "-c",
                "credential.helper=",
                "-c",
                "credential.helper=!gh auth git-credential",
                "push",
                &format!("--force-with-lease=refs/heads/{RELEASE_BRANCH}:{previous_sha}"),
                "origin",
                &format!("HEAD:refs/heads/{RELEASE_BRANCH}"),
            ])?;
        }
        let notes = files::notes(&changelog, &version)?;
        let replacement_note = replacement.map_or_else(String::new, |replaced| {
            format!(
                r#"
{REPLACEMENT_PREFIX}{replaced} -->

Replaces unpublished source `{replaced}` at the same version.
Merging this PR approves the corrected source.
"#
            )
        });
        let refresh_note = if replacement.is_some() {
            "If the default branch advances, dispatch replacement again with the same pending source SHA."
        } else {
            "Further default-branch merges refresh this PR. Wait for its refreshed checks before merging."
        };
        let body = format!(
            r#"{MARKER}{replacement_note}

{notes}

Prepared from `{commit}` with Convco.

Merge after the required checks pass to build and publish the release.

{refresh_note}
"#
        );
        let payload = json!({"head":RELEASE_BRANCH, "base":self.branch, "title":format!("chore(release): {tag}"), "body":body});
        let pr = if let Some(pr) = existing.as_ref() {
            if pr["title"] == payload["title"] && pr["body"] == payload["body"] {
                pr.clone()
            } else {
                self.github
                    .write("PATCH", &format!("pulls/{}", pr["number"]), payload)?
            }
        } else {
            self.github.write("POST", "pulls", payload)?
        };
        report::note(&format!("Release PR: {}", text(&pr, "html_url")?))?;
        Ok(
            json!({"pr-url":pr["html_url"], "pr-number":pr["number"], "version":version.to_string(), "tag":tag}),
        )
    }

    pub fn publish(&self, commit: &str, run: &str, manifest: &str) -> Result<Value> {
        self.require_current(commit)?;
        let pr = self
            .pull(commit)?
            .context("publication requires a merged managed release PR")?;
        let version = self.verify(commit, &pr)?;
        let tag = format!("v{version}");
        if let Some(target) = self.tag_target(&tag)? {
            ensure!(
                target == commit,
                "immutable version tag points to another commit"
            );
        }
        let bundle = artifacts::Bundle::download(self, run, manifest, commit, &version)?;
        self.require_current(commit)?;
        let mut notes = files::notes(&project::file_at(commit, "CHANGELOG.md")?, &version)?;
        notes.push_str(&bundle.notes);
        let releases = self.releases()?;
        let existing = releases.iter().find(|release| release["tag_name"] == tag);
        let release = if let Some(release) = existing {
            ensure!(
                release["prerelease"] == false,
                "existing release is unexpectedly a prerelease"
            );
            ensure!(
                self.tag_target(&tag)?.is_some() || release["target_commitish"] == commit,
                "existing draft targets another commit; refusing to publish it"
            );
            release.clone()
        } else {
            self.github.write("POST", "releases", json!({"tag_name":tag, "target_commitish":commit, "name":tag, "body":notes, "draft":true, "prerelease":false}))?
        };
        let id = release["id"].as_u64().context("release id")?;
        let draft = release["draft"] == true;
        bundle.attach(&self.github, id, &tag, draft)?;
        let release = if draft {
            self.github.write(
                "PATCH",
                &format!("releases/{id}"),
                json!({"body":notes,"draft":false,"make_latest":"true"}),
            )?
        } else {
            release
        };
        // Confirm the automatic tag creation used the approved commit, including on retries.
        git(&[
            "fetch",
            "--force",
            "origin",
            &format!("refs/tags/{tag}:refs/tags/{tag}"),
        ])?;
        ensure!(
            self.tag_target(&tag)?.as_deref() == Some(commit),
            "published release tag differs from the approved commit"
        );
        let url = text(&release, "html_url")?;
        report::note(&format!("Published {url}"))?;
        Ok(json!({"release-url":url,"tag":tag,"version":version.to_string(),"commit-sha":commit}))
    }
}

fn commit_prepared(commit: &str, tag: &str, previous_sha: &str) -> Result<bool> {
    let unchanged = !previous_sha.is_empty()
        && git(&["rev-parse", &format!("{previous_sha}^")])? == commit
        && git(&["rev-parse", &format!("{previous_sha}^{{tree}}")])? == git(&["write-tree"])?;
    if !unchanged {
        git(&[
            "-c",
            "user.name=github-actions[bot]",
            "-c",
            "user.email=41898282+github-actions[bot]@users.noreply.github.com",
            "commit",
            "-m",
            &format!("chore(release): {tag}"),
        ])?;
    }
    Ok(unchanged)
}
