use anyhow::{Context, Result, ensure};
use semver::Version;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{git, github::Github, project};

#[derive(Deserialize)]
struct Pull {
    number: u64,
    merged_at: Option<String>,
    merge_commit_sha: Option<String>,
    head: Branch,
    base: Branch,
}

#[derive(Deserialize)]
struct Branch {
    sha: String,
    #[serde(rename = "ref")]
    name: String,
    repo: Option<Repository>,
}

#[derive(Deserialize)]
struct Repository {
    full_name: String,
}

fn release_pull<'a>(pulls: &'a [Pull], repo: &str, commit: &str) -> Result<&'a Pull> {
    let matches: Vec<_> = pulls
        .iter()
        .filter(|pr| {
            pr.merged_at.is_some()
                && pr.merge_commit_sha.as_deref() == Some(commit)
                && pr.head.name == "release/next"
                && pr.base.name == "trunk"
                && pr.head.repo.as_ref().is_some_and(|r| r.full_name == repo)
                && pr.base.repo.as_ref().is_some_and(|r| r.full_name == repo)
        })
        .collect();
    ensure!(
        matches.len() == 1,
        "commit must be the merge of one release/next PR into this repository's trunk"
    );
    Ok(matches[0])
}

fn ci_succeeded(runs: &Value, repo: &str, commit: &str) -> Result<()> {
    // The API returns newest runs first. A later failure must invalidate an older success.
    let latest = runs["workflow_runs"]
        .as_array()
        .context("CI workflow runs")?
        .iter()
        .find(|run| {
            run["head_sha"] == commit
                && run["head_branch"] == "trunk"
                && run["event"] == "push"
                && run["head_repository"]["full_name"] == repo
        })
        .context("no trunk push CI run for this exact commit; wait for CI")?;
    ensure!(
        latest["status"] == "completed" && latest["conclusion"] == "success",
        "latest CI for this trunk commit has not succeeded"
    );
    Ok(())
}

fn tag_target(tag: &str) -> Result<Option<String>> {
    if git(&["tag", "--list", tag])?.is_empty() {
        Ok(None)
    } else {
        Ok(Some(git(&[
            "rev-parse",
            &format!("refs/tags/{tag}^{{commit}}"),
        ])?))
    }
}

fn matching_tag(existing: Option<&str>, commit: &str) -> Result<()> {
    ensure!(
        existing.is_none_or(|sha| sha == commit),
        "version tag already points to a different commit; immutable tags cannot be moved"
    );
    Ok(())
}

pub(crate) fn verify_candidate(commit: &str, head: &str) -> Result<Version> {
    let base = git(&["rev-parse", &format!("{head}^")])?;
    ensure!(
        base == git(&["rev-parse", &format!("{commit}^1")])?,
        "stale release PR: trunk advanced before merging; prepare a fresh release PR"
    );
    ensure!(
        git(&["rev-parse", &format!("{head}^{{tree}}")])?
            == git(&["rev-parse", &format!("{commit}^{{tree}}")])?,
        "merged release differs from the reviewed release PR"
    );
    project::release_changes(&base, head)?;
    let manifest = project::file_at(commit, "Cargo.toml")?;
    let version = project::version(&manifest)?;
    let calculated = project::next_version(&base, &project::file_at(commit, ".convco")?)?;
    ensure!(
        version == calculated,
        "Cargo.toml is {version}, but Convco calculated {calculated}; prepare a fresh release PR"
    );
    let expected_manifest =
        project::set_version(&project::file_at(&base, "Cargo.toml")?, &version)?;
    ensure!(
        manifest.trim_end() == expected_manifest.trim_end(),
        "release PR changed Cargo.toml beyond its version"
    );
    project::lock_versions(&project::file_at(commit, "Cargo.lock")?, &version)?;
    let expected_lock =
        project::set_lock_versions(&project::file_at(&base, "Cargo.lock")?, &version)?;
    ensure!(
        project::file_at(commit, "Cargo.lock")?.trim_end() == expected_lock.trim_end(),
        "release PR changed dependency resolution beyond workspace versions"
    );
    let changelog = project::file_at(commit, "CHANGELOG.md")?;
    ensure!(
        changelog.lines().find_map(project::heading_version) == Some(version.clone()),
        "changelog does not start with the release version"
    );
    Ok(version)
}

pub(crate) fn execute(github: &Github, commit: &str) -> Result<()> {
    project::full_sha(commit)?;
    git(&["fetch", "origin", "trunk", "--tags"])?;
    git(&["merge-base", "--is-ancestor", commit, "origin/trunk"])?;
    let pulls: Vec<Pull> = github.get(&format!("commits/{commit}/pulls?per_page=100"))?;
    let pr = release_pull(&pulls, &github.repo, commit)?;
    // Keep PR commits available even when GitHub deletes release/next after merging.
    git(&["fetch", "origin", &format!("refs/pull/{}/head", pr.number)])?;
    ensure!(
        git(&["rev-parse", "FETCH_HEAD"])? == pr.head.sha,
        "release PR head changed"
    );
    let version = verify_candidate(commit, &pr.head.sha)?;
    let runs: Value = github.get(&format!(
        "actions/workflows/ci.yml/runs?head_sha={commit}&event=push&branch=trunk&per_page=100"
    ))?;
    ci_succeeded(&runs, &github.repo, commit)?;
    let tag = format!("v{version}");
    let existing = tag_target(&tag)?;
    matching_tag(existing.as_deref(), commit)?;
    // Never publish out of order or move a consumer alias backwards.
    for other in git(&["tag", "--list", "v*"])?.lines() {
        if let Ok(other_version) = Version::parse(other.trim_start_matches('v')) {
            ensure!(
                other_version <= version,
                "newer version {other} already exists"
            );
        }
    }
    let alias = format!("v{}", version.major);
    if let Some(old) = tag_target(&alias)? {
        git(&["merge-base", "--is-ancestor", &old, commit])?;
    }
    if existing.is_none() {
        github.create_ref(&tag, commit)?;
    }
    // Query all releases so an API/network failure is never mistaken for absence.
    let releases = crate::run(std::process::Command::new("gh").args([
        "api",
        "--paginate",
        "--slurp",
        &format!("repos/{}/releases?per_page=100", github.repo),
    ]))?;
    let pages: Vec<Vec<Value>> = serde_json::from_str(&releases)?;
    let release = pages
        .iter()
        .flatten()
        .find(|release| release["tag_name"] == tag);
    let body = project::notes(
        &github.repo,
        &tag,
        &project::file_at(commit, "CHANGELOG.md")?,
    );
    let payload = json!({"tag_name": tag, "target_commitish": commit, "name": tag, "body": body, "draft": false, "prerelease": false, "make_latest": "true"});
    match release {
        Some(release) if release["draft"] == true => {
            github.write("PATCH", &format!("releases/{}", release["id"]), payload)?;
        }
        Some(release) => {
            ensure!(
                release["prerelease"] == false,
                "existing release is unexpectedly a prerelease"
            );
        }
        None => {
            github.write("POST", "releases", payload)?;
        }
    }
    if tag_target(&alias)?.is_some() {
        github.write(
            "PATCH",
            &format!("git/refs/tags/{alias}"),
            json!({"sha": commit, "force": true}),
        )?;
    } else {
        github.create_ref(&alias, commit)?;
    }
    println!(
        "Published https://github.com/{}/releases/tag/{tag}",
        github.repo
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_latest_successful_push_for_exact_sha_and_repository() {
        let good = json!({"head_sha": "abc", "head_branch": "trunk", "event": "push", "head_repository": {"full_name": "owner/repo"}, "status": "completed", "conclusion": "success"});
        assert!(
            ci_succeeded(
                &json!({"workflow_runs": [good.clone()]}),
                "owner/repo",
                "abc"
            )
            .is_ok()
        );
        for (key, bad) in [
            ("head_sha", "other"),
            ("event", "pull_request"),
            ("status", "in_progress"),
            ("conclusion", "failure"),
            ("head_branch", "release/next"),
        ] {
            let mut run = good.clone();
            run[key] = json!(bad);
            assert!(ci_succeeded(&json!({"workflow_runs": [run]}), "owner/repo", "abc").is_err());
        }
        let mut failed = good.clone();
        failed["conclusion"] = json!("failure");
        assert!(
            ci_succeeded(
                &json!({"workflow_runs": [failed, good]}),
                "owner/repo",
                "abc"
            )
            .is_err()
        );
    }

    #[test]
    fn requires_merged_release_pr_from_same_repository() {
        let good = json!({"number": 2, "merged_at": "date", "merge_commit_sha": "abc", "head": {"sha": "def", "ref": "release/next", "repo": {"full_name": "owner/repo"}}, "base": {"sha": "base", "ref": "trunk", "repo": {"full_name": "owner/repo"}}});
        assert!(
            release_pull(
                &[serde_json::from_value(good.clone()).unwrap()],
                "owner/repo",
                "abc"
            )
            .is_ok()
        );
        for pointer in [
            "/merged_at",
            "/merge_commit_sha",
            "/head/repo",
            "/base/repo",
        ] {
            let mut bad = good.clone();
            *bad.pointer_mut(pointer).unwrap() = Value::Null;
            assert!(
                release_pull(&[serde_json::from_value(bad).unwrap()], "owner/repo", "abc").is_err()
            );
        }
    }

    #[test]
    fn retries_allow_only_the_same_immutable_tag() {
        assert!(matching_tag(None, "abc").is_ok());
        assert!(matching_tag(Some("abc"), "abc").is_ok());
        assert!(matching_tag(Some("other"), "abc").is_err());
    }
}
