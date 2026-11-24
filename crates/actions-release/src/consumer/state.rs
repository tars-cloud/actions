use anyhow::{Context, Result, ensure};
use semver::Version;
use serde_json::{Value, json};
use std::fs;

use super::git;
use super::{PendingReleasePolicy, artifacts, decision, files, numeric, skipped, text};
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

    fn published_at_or_after(&self, commit: &str) -> Result<bool> {
        let version = files::version(&project::file_at(commit, "Cargo.toml")?)?;
        let tag = format!("v{version}");
        if let Some(target) = self.tag_target(&tag)? {
            ensure!(
                target == commit || git(&["merge-base", "--is-ancestor", commit, &target]).is_ok(),
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

    fn has_blocking_release(&self, tag: &str, replaced: &str) -> Result<bool> {
        // Draft visibility must match the identity that can prepare or publish the release.
        let releases: Vec<Value> = self.github.list_with_write_token("releases?per_page=100")?;
        Ok(releases.iter().any(|release| {
            if release["tag_name"] != tag {
                return false;
            }
            // Only an untagged draft for the pending commit itself can be cleanly replaced.
            !(release["draft"] == true
                && release["prerelease"] != true
                && release["target_commitish"].as_str() == Some(replaced))
        }))
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
            if self.published_at_or_after(commit)? {
                return Ok(None);
            }
            return Ok(Some(commit.to_owned()));
        }
        Ok(None)
    }

    fn replacement_version(
        &self,
        commit: &str,
        replaced: &str,
        automatic: bool,
    ) -> Result<Version> {
        ensure!(
            !self.recovery
                && (automatic
                    || std::env::var("GITHUB_EVENT_NAME").as_deref() == Ok("workflow_dispatch")),
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
        if commit == replaced {
            let stale_or_invalid = automatic
                && self
                    .pull(replaced)?
                    .is_some_and(|pr| self.verify(replaced, &pr).is_err());
            ensure!(
                stale_or_invalid,
                "replacement requires newer reviewed source"
            );
        } else {
            git(&["merge-base", "--is-ancestor", replaced, commit])?;
        }
        let pending_version = files::version(&project::file_at(replaced, "Cargo.toml")?)?;
        let pending_tag = format!("v{pending_version}");
        ensure!(
            self.tag_target(&pending_tag)?.is_none(),
            "tagged releases cannot be replaced"
        );
        ensure!(
            !self.has_blocking_release(&pending_tag, replaced)?,
            "existing releases or drafts must be recovered from their original source"
        );
        let version = files::next_version(commit, &pending_version)?;
        let tag = format!("v{version}");
        ensure!(
            self.tag_target(&tag)?.is_none(),
            "tagged releases cannot be replaced"
        );
        ensure!(
            !self.has_blocking_release(&tag, replaced)?,
            "existing releases or drafts must be recovered from their original source"
        );
        Ok(version)
    }

    fn pending_run(&self, commit: &str) -> Result<(String, bool)> {
        let mut run_url = String::new();
        let mut in_flight = false;
        if let Ok(run_id) = std::env::var("GITHUB_RUN_ID") {
            numeric(&run_id)?;
            let run: Value = self.github.get(&format!("actions/runs/{run_id}"))?;
            let workflow = run["workflow_id"].as_u64().context("caller workflow id")?;
            let runs: Value = self.github.get(&format!(
                "actions/workflows/{workflow}/runs?head_sha={commit}&event=push&per_page=100"
            ))?;
            let runs = runs["workflow_runs"].as_array().context("workflow runs")?;
            if let Some(run) = runs
                .iter()
                .find(|run| run["head_sha"] == commit && run["event"] == "push")
            {
                run_url = text(run, "html_url")?.to_owned();
                in_flight = run["status"].as_str().is_some_and(|status| {
                    matches!(
                        status,
                        "queued" | "in_progress" | "waiting" | "pending" | "requested"
                    )
                });
            }
        }
        Ok((run_url, in_flight))
    }

    fn with_pending_run(&self, mut result: Value, commit: &str, run_url: &str) -> Result<Value> {
        let version = files::version(&project::file_at(commit, "Cargo.toml")?)?;
        result["pending-commit-sha"] = json!(commit);
        result["pending-version"] = json!(version.to_string());
        result["pending-run-url"] = json!(run_url);
        Ok(result)
    }

    fn with_pending(&self, result: Value, commit: &str) -> Result<Value> {
        let (run_url, _) = self.pending_run(commit)?;
        self.with_pending_run(result, commit, &run_url)
    }

    fn pending_pr_valid(&self, pending: &str) -> Result<Option<(Value, Version)>> {
        let Some(pr) = self.pull(pending)? else {
            return Ok(None);
        };
        let number = pr["number"].as_u64().context("release PR number")?;
        let head = text(&pr["head"], "sha")?;
        project::full_sha(head)?;
        git(&["fetch", "origin", &format!("refs/pull/{number}/head")])?;
        if git(&["rev-parse", "FETCH_HEAD"])? != head {
            return Ok(None);
        }
        let base = git(&["rev-parse", &format!("{head}^")])?;
        if base != git(&["rev-parse", &format!("{pending}^1")])?
            || git(&["rev-parse", &format!("{head}^{{tree}}")])?
                != git(&["rev-parse", &format!("{pending}^{{tree}}")])?
        {
            return Ok(None);
        }
        let version = files::version(&project::file_at(pending, "Cargo.toml")?)?;
        Ok(Some((pr, version)))
    }

    fn direct_publish_candidate(
        &self,
        commit: &str,
        pending: &str,
    ) -> Result<Option<(Value, Version)>> {
        if self.recovery || commit == pending {
            return Ok(None);
        }
        if git(&["merge-base", "--is-ancestor", pending, commit]).is_err() {
            return Ok(None);
        }
        let Some((pr, pending_version)) = self.pending_pr_valid(pending)? else {
            return Ok(None);
        };
        let pending_tag = format!("v{pending_version}");
        if self.tag_target(&pending_tag)?.is_some()
            || self.has_blocking_release(&pending_tag, pending)?
        {
            return Ok(None);
        }
        let version = files::next_version(commit, &pending_version)?;
        if version != pending_version {
            return Ok(None);
        }
        let project = files::Project::read()?;
        if !project.matches_prepared(commit, &version)? {
            return Ok(None);
        }
        for tag in git(&["tag", "--list", "v*"])?.lines() {
            if let Ok(other) = Version::parse(tag.trim_start_matches('v'))
                && other > version
            {
                return Ok(None);
            }
        }
        Ok(Some((pr, version)))
    }

    pub fn recover_pending(&self) -> Result<Value> {
        ensure!(
            std::env::var("GITHUB_EVENT_NAME").as_deref() == Ok("workflow_dispatch"),
            "pending recovery requires a manual default-branch dispatch"
        );
        let Some(commit) = self.pending()? else {
            return Ok(decision(
                "no-pending-release",
                "No managed release is awaiting publication.",
            ));
        };
        git(&["merge-base", "--is-ancestor", &commit, &self.trigger])?;
        // Verification reads Cargo metadata from the approved source, not the newer workflow checkout.
        git(&["checkout", "--detach", &commit])?;
        let repository = Self::open(&commit, true)?;
        let result = repository.candidate(&commit, None, PendingReleasePolicy::Wait)?;
        repository.with_pending(result, &commit)
    }

    pub fn candidate(
        &self,
        commit: &str,
        replacement: Option<&str>,
        policy: PendingReleasePolicy,
    ) -> Result<Value> {
        if let Some(replaced) = replacement {
            let version = self.replacement_version(commit, replaced, false)?;
            return self.with_pending(
                json!({"decision":"refresh-unpublished", "reason":"Prepare a replacement PR from the current default branch for fresh approval.", "prepare-ready":"true", "release-ready":"false", "commit-sha":commit, "version":version.to_string(), "tag":format!("v{version}")}),
                replaced,
            );
        }
        if let Some(pr) = self.pull(commit)? {
            return self.candidate_for_release_pr(commit, &pr, policy);
        }
        if let Some(pending) = self.pending()? {
            return self.candidate_with_pending(commit, &pending, policy);
        }
        if commit != self.tip {
            return Ok(decision(
                "obsolete",
                "Default branch advanced; the newer run will prepare the release.",
            ));
        }
        let initial = files::version(&project::file_at(commit, "Cargo.toml")?)?;
        let next = files::next_version(commit, &initial)?;
        if self.tag_target(&format!("v{next}"))?.is_some() {
            return Ok(skipped(
                "No version bump: no releasable conventional commits.",
            ));
        }
        Ok(
            json!({"decision":"prepare", "reason":"Refresh the release PR from the current default branch.", "prepare-ready":"true", "release-ready":"false", "commit-sha":commit, "version":next.to_string(), "tag":format!("v{next}")}),
        )
    }

    fn candidate_for_release_pr(
        &self,
        commit: &str,
        pr: &Value,
        policy: PendingReleasePolicy,
    ) -> Result<Value> {
        let declared = files::version(&project::file_at(commit, "Cargo.toml")?)?;
        if self.superseded_by_descendant_tag(commit, &declared)? {
            return Ok(decision(
                "superseded",
                "A newer managed release replaced this candidate.",
            ));
        }
        if self.published(commit)? {
            return Ok(decision("published", "Release already published."));
        }
        if self.pending()?.as_deref() != Some(commit) {
            return Ok(decision(
                "superseded",
                "A newer managed release replaced this candidate.",
            ));
        }
        match self.verify(commit, pr) {
            Ok(version) => Ok(
                json!({"decision":"release", "reason":"Build and verify the approved release source before publication.", "recovery":self.recovery.to_string(), "prepare-ready":"false", "release-ready":"true", "version":version.to_string(), "tag":format!("v{version}"), "commit-sha":commit, "pr-number":pr["number"]}),
            ),
            Err(error) => self.handle_invalid_release_pr(commit, policy, error),
        }
    }

    fn superseded_by_descendant_tag(&self, commit: &str, declared: &Version) -> Result<bool> {
        let Some(target) = self.tag_target(&format!("v{declared}"))? else {
            return Ok(false);
        };
        Ok(target != commit && git(&["merge-base", "--is-ancestor", commit, &target]).is_ok())
    }

    fn handle_invalid_release_pr(
        &self,
        commit: &str,
        policy: PendingReleasePolicy,
        error: anyhow::Error,
    ) -> Result<Value> {
        if policy == PendingReleasePolicy::RefreshUnpublished
            && !self.recovery
            && commit == self.tip
            && let Ok(version) = self.replacement_version(commit, commit, true)
        {
            return self.with_pending(
                json!({"decision":"refresh-unpublished", "reason":format!("Merged release candidate could not be published ({error:#}); prepare a fresh release PR from the current default branch."), "prepare-ready":"true", "release-ready":"false", "commit-sha":commit, "version":version.to_string(), "tag":format!("v{version}")}),
                commit,
            );
        }
        Err(error)
    }

    fn candidate_with_pending(
        &self,
        commit: &str,
        pending: &str,
        policy: PendingReleasePolicy,
    ) -> Result<Value> {
        let (run_url, in_flight) = self.pending_run(pending)?;
        let result = self.with_pending_run(
            decision(
                "pending-release",
                &format!("Release {pending} is awaiting publication; rerun its release workflow."),
            ),
            pending,
            &run_url,
        )?;
        if policy != PendingReleasePolicy::RefreshUnpublished || in_flight || commit != self.tip {
            return Ok(result);
        }
        self.try_refresh_pending(commit, pending, result)
    }

    fn try_refresh_pending(&self, commit: &str, pending: &str, mut result: Value) -> Result<Value> {
        let pending_version = files::version(&project::file_at(pending, "Cargo.toml")?)?;
        let pending_tag = format!("v{pending_version}");
        if self.tag_target(&pending_tag)?.is_some()
            || self.has_blocking_release(&pending_tag, pending)?
        {
            return Ok(result);
        }
        let version = files::next_version(commit, &pending_version)?;
        let tag = format!("v{version}");
        if self.tag_target(&tag)?.is_some() || self.has_blocking_release(&tag, pending)? {
            return Ok(result);
        }
        if let Some((pr, version)) = self.direct_publish_candidate(commit, pending)? {
            result["decision"] = json!("release");
            result["reason"] = json!(
                "Release metadata already matches the approved unpublished version; build and verify before publication."
            );
            result["recovery"] = json!("false");
            result["release-ready"] = json!("true");
            result["commit-sha"] = json!(commit);
            result["version"] = json!(version.to_string());
            result["tag"] = json!(format!("v{version}"));
            result["pr-number"] = pr["number"].clone();
        } else {
            result["decision"] = json!("refresh-unpublished");
            result["reason"] = json!(
                "Prepare a replacement PR from the current default branch for fresh approval."
            );
            result["prepare-ready"] = json!("true");
            result["commit-sha"] = json!(commit);
            result["version"] = json!(version.to_string());
            result["tag"] = json!(tag);
        }
        Ok(result)
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

    pub fn prepare(
        &self,
        commit: &str,
        replacement: Option<&str>,
        policy: PendingReleasePolicy,
    ) -> Result<Value> {
        let candidate = self.candidate(commit, replacement, policy)?;
        if candidate["prepare-ready"] != "true" {
            return Ok(candidate);
        }
        let replacement = replacement.or_else(|| candidate["pending-commit-sha"].as_str());
        let project = files::Project::read()?;
        let Some(version) = self.prepare_version(commit, &project, replacement, policy)? else {
            return Ok(skipped(
                "No version bump: no releasable conventional commits.",
            ));
        };
        let tag = format!("v{version}");
        let existing = self.open_pull()?;
        let changelog = match self.stage_and_push_release(commit, &project, &version, &tag)? {
            StageOutcome::NoDiff => {
                return Ok(skipped(
                    "Release metadata already matches the calculated version; no PR changes are needed.",
                ));
            }
            StageOutcome::BranchAdvanced => {
                return Ok(skipped(
                    "Default branch advanced during preparation; a newer run will refresh the PR.",
                ));
            }
            StageOutcome::Ready(changelog) => changelog,
        };
        let body = self.prepare_pr_body(commit, &version, &changelog, replacement)?;
        let pr = self.refresh_pull(existing.as_ref(), &tag, &body)?;
        report::note(&format!("Release PR: {}", text(&pr, "html_url")?))?;
        Ok(
            json!({"pr-url":pr["html_url"], "pr-number":pr["number"], "version":version.to_string(), "tag":tag}),
        )
    }

    fn prepare_version(
        &self,
        commit: &str,
        project: &files::Project,
        replacement: Option<&str>,
        policy: PendingReleasePolicy,
    ) -> Result<Option<Version>> {
        let version = if let Some(replaced) = replacement {
            self.replacement_version(
                commit,
                replaced,
                policy == PendingReleasePolicy::RefreshUnpublished,
            )?
        } else {
            files::next_version(commit, &project.version)?
        };
        if self.tag_target(&format!("v{version}"))?.is_some() {
            return Ok(None);
        }
        ensure!(
            replacement.is_some() || version >= project.version,
            "Convco calculated a version older than Cargo.toml"
        );
        Ok(Some(version))
    }

    fn stage_and_push_release(
        &self,
        commit: &str,
        project: &files::Project,
        version: &Version,
        tag: &str,
    ) -> Result<StageOutcome> {
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
        let changelog = project.write(commit, version)?;
        let mut paths: Vec<&str> = vec!["add", "--", "Cargo.lock", "CHANGELOG.md"];
        paths.extend(project.manifests.iter().map(String::as_str));
        git(&paths)?;
        if git(&["diff", "--cached", "--name-only"])?.is_empty() {
            return Ok(StageOutcome::NoDiff);
        }
        let unchanged = commit_prepared(commit, tag, previous_sha)?;
        let remote = git(&[
            "ls-remote",
            "origin",
            &format!("refs/heads/{}", self.branch),
        ])?;
        if remote.split_whitespace().next() != Some(commit) {
            return Ok(StageOutcome::BranchAdvanced);
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
        Ok(StageOutcome::Ready(changelog))
    }

    fn prepare_pr_body(
        &self,
        commit: &str,
        version: &Version,
        changelog: &str,
        replacement: Option<&str>,
    ) -> Result<String> {
        let notes = files::notes(changelog, version)?;
        let replacement_note = match replacement {
            Some(replaced) => {
                let pending_version = files::version(&project::file_at(replaced, "Cargo.toml")?)?;
                let summary = if pending_version == *version {
                    format!("Replaces unpublished source `{replaced}` at the same version.")
                } else {
                    format!(
                        "Replaces unpublished source `{replaced}` with recalculated version `{version}`."
                    )
                };
                format!(
                    r#"
{REPLACEMENT_PREFIX}{replaced} -->

{summary}
Merging this PR approves the corrected source.
"#
                )
            }
            None => String::new(),
        };
        let refresh_note = if replacement.is_some() {
            "Further default-branch merges refresh this PR automatically unless pending-release-policy is set to wait."
        } else {
            "Further default-branch merges refresh this PR. Wait for its refreshed checks before merging."
        };
        Ok(format!(
            r#"{MARKER}{replacement_note}

{notes}

Prepared from `{commit}` with Convco.

Merge after the required checks pass to build and publish the release.

{refresh_note}
"#
        ))
    }

    fn refresh_pull(&self, existing: Option<&Value>, tag: &str, body: &str) -> Result<Value> {
        let title = format!("chore(release): {tag}");
        if let Some(pr) = existing {
            if pr["title"] == title && pr["body"] == body {
                Ok(pr.clone())
            } else {
                self.github.write(
                    "PATCH",
                    &format!("pulls/{}", pr["number"]),
                    json!({"title": title, "body": body}),
                )
            }
        } else {
            self.github.write(
                "POST",
                "pulls",
                json!({"head": RELEASE_BRANCH, "base": self.branch, "title": title, "body": body}),
            )
        }
    }

    pub fn publish(&self, commit: &str, run: &str, manifest: &str) -> Result<Value> {
        let managed_pr = self.pull(commit)?;
        let version = self.resolve_publish_version(commit, managed_pr.as_ref())?;
        let tag = format!("v{version}");
        let bundle = artifacts::Bundle::download(self, run, manifest, commit, &version)?;
        self.recheck_publish_eligibility(commit, managed_pr.as_ref())?;
        let mut notes = files::notes(&project::file_at(commit, "CHANGELOG.md")?, &version)?;
        notes.push_str(&bundle.notes);
        let release = self.ensure_publish_release(commit, &tag, &notes)?;
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

    fn resolve_publish_version(&self, commit: &str, managed_pr: Option<&Value>) -> Result<Version> {
        let version = if let Some(pr) = managed_pr {
            self.require_current(commit)?;
            self.verify(commit, pr)?
        } else if self.published(commit)? {
            files::version(&project::file_at(commit, "Cargo.toml")?)?
        } else {
            let pending = self
                .pending()?
                .context("publication requires a merged managed release PR")?;
            let (_, version) = self
                .direct_publish_candidate(commit, &pending)?
                .context("publication requires a merged managed release PR")?;
            version
        };
        let tag = format!("v{version}");
        if let Some(target) = self.tag_target(&tag)? {
            ensure!(
                target == commit,
                "immutable version tag points to another commit"
            );
        }
        Ok(version)
    }

    fn recheck_publish_eligibility(&self, commit: &str, managed_pr: Option<&Value>) -> Result<()> {
        if managed_pr.is_some() {
            return self.require_current(commit);
        }
        if self.published(commit)? {
            return Ok(());
        }
        let pending = self
            .pending()?
            .context("publication requires the latest approved managed release")?;
        ensure!(
            self.direct_publish_candidate(commit, &pending)?.is_some(),
            "publication requires the latest approved managed release"
        );
        Ok(())
    }

    fn ensure_publish_release(&self, commit: &str, tag: &str, notes: &str) -> Result<Value> {
        let releases: Vec<Value> = self.github.list_with_write_token("releases?per_page=100")?;
        let Some(release) = releases.iter().find(|release| release["tag_name"] == tag) else {
            return self.create_draft_release(commit, tag, notes);
        };
        ensure!(
            release["prerelease"] == false,
            "existing release is unexpectedly a prerelease"
        );
        let tagged = self.tag_target(tag)?.is_some();
        if !tagged
            && release["draft"] == true
            && release["target_commitish"].as_str() != Some(commit)
        {
            let target = release["target_commitish"]
                .as_str()
                .context("existing draft target commit")?;
            ensure!(
                project::full_sha(target).is_ok()
                    && git(&["merge-base", "--is-ancestor", target, commit]).is_ok(),
                "existing draft targets another commit; refusing to publish it"
            );
            let old_id = release["id"].as_u64().context("release id")?;
            self.github.delete(&format!("releases/{old_id}"))?;
            return self.create_draft_release(commit, tag, notes);
        }
        ensure!(
            tagged || release["target_commitish"] == commit,
            "existing draft targets another commit; refusing to publish it"
        );
        Ok(release.clone())
    }

    fn create_draft_release(&self, commit: &str, tag: &str, notes: &str) -> Result<Value> {
        self.github.write(
            "POST",
            "releases",
            json!({"tag_name":tag, "target_commitish":commit, "name":tag, "body":notes, "draft":true, "prerelease":false}),
        )
    }
}

enum StageOutcome {
    NoDiff,
    BranchAdvanced,
    Ready(String),
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
