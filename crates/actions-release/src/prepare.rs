use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::{git, github::Github, project, publish, report, run};

pub(crate) fn execute(github: &Github, automatic: bool) -> Result<()> {
    ensure!(
        git(&["status", "--porcelain", "--untracked-files=no"])?.is_empty(),
        "tracked checkout must be clean"
    );
    crate::fetch_trunk()?;
    let base = git(&["rev-parse", "origin/trunk"])?;
    if automatic && !publish::ci_ready(github, &base)? {
        return report::note(&format!(
            "Awaiting successful trunk CI for `{base}` before refreshing the release PR."
        ));
    }
    if let Some((commit, head)) = publish::pending_release(github)? {
        if publish::verify_candidate(&commit, &head).is_ok() {
            return report::note(&format!(
                "Awaiting publication of merged release `{commit}`; no second release PR will be created. If publication failed, retry it with Repository: Release Automation."
            ));
        }
        report::note("Rebuilding an invalid merged release candidate from current trunk.")?;
    }
    let version = project::next_version(&base, &project::file_at(&base, ".convco")?)?;
    let tag = format!("v{version}");
    let tags = git(&["tag", "--list", &tag])?;
    if !tags.is_empty() {
        return report::note(&format!(
            "Nothing to release: {tag} already exists and there are no releasable changes."
        ));
    }

    let prs = open_prs(github)?;
    let previous = git(&["ls-remote", "origin", "refs/heads/release/next"])?;
    let previous_sha = previous.split_whitespace().next().unwrap_or("");
    if !previous_sha.is_empty() {
        git(&["fetch", "origin", "refs/heads/release/next"])?;
    }
    git(&["checkout", "-B", "release/next", &base])?;
    let changelog = write_files(Path::new("."), &base, &version)?;
    git(&["add", "--", "Cargo.toml", "Cargo.lock", "CHANGELOG.md"])?;
    if git(&["diff", "--cached", "--name-only"])?.is_empty() {
        return report::note(
            "Release is already prepared; awaiting publication of the merged release commit.",
        );
    }
    let (unchanged, head) = commit_prepared(&base, &tag, previous_sha)?;
    if base
        != git(&["ls-remote", "origin", "refs/heads/trunk"])?
            .split_whitespace()
            .next()
            .context("remote trunk")?
    {
        return report::note(
            "Trunk advanced during preparation; its CI completion will refresh the release PR.",
        );
    }
    if !unchanged {
        git(&[
            "-c",
            "credential.helper=",
            "-c",
            "credential.helper=!gh auth git-credential",
            "push",
            &format!("--force-with-lease=refs/heads/release/next:{previous_sha}"),
            "origin",
            "HEAD:refs/heads/release/next",
        ])?;
    }
    let notes = project::notes(&github.repo, &tag, &changelog).replace(
        &format!("/blob/{tag}/CHANGELOG.md"),
        &format!("/blob/{head}/CHANGELOG.md"),
    );
    let body = format!(
        r#"{notes}
Prepared from `{base}` by Convco.

Merge after review and CI. Successful trunk CI publishes the merged release automatically.

Use **Repository: Release Automation** manually only to retry publication.

Further successful trunk CI refreshes this PR. Wait for its refreshed checks before merging.
"#
    );
    refresh_pr(github, &prs, &tag, &body, unchanged)
}

fn open_prs(github: &Github) -> Result<Vec<Value>> {
    let owner = github.repo.split('/').next().context("repository owner")?;
    let prs: Vec<Value> = github.list(&format!(
        "pulls?state=open&base=trunk&head={owner}:release/next&per_page=100"
    ))?;
    ensure!(prs.len() <= 1, "multiple release PRs exist");
    for pr in &prs {
        ensure!(
            pr["head"]["repo"]["full_name"] == github.repo,
            "release PR must belong to this repository"
        );
    }
    Ok(prs)
}

fn commit_prepared(base: &str, tag: &str, previous_sha: &str) -> Result<(bool, String)> {
    let unchanged = !previous_sha.is_empty()
        && git(&["rev-parse", &format!("{previous_sha}^")])? == base
        && git(&["rev-parse", &format!("{previous_sha}^{{tree}}")])? == git(&["write-tree"])?;
    let head = if unchanged {
        previous_sha.to_owned()
    } else {
        git(&[
            "-c",
            "user.name=github-actions[bot]",
            "-c",
            "user.email=41898282+github-actions[bot]@users.noreply.github.com",
            "commit",
            "-m",
            &format!("chore(release): {tag}"),
        ])?;
        project::release_changes(base, "HEAD")?;
        git(&["rev-parse", "HEAD"])?
    };
    Ok((unchanged, head))
}

fn refresh_pr(
    github: &Github,
    prs: &[Value],
    tag: &str,
    body: &str,
    unchanged: bool,
) -> Result<()> {
    if unchanged
        && let Some(pr) = prs.first()
        && pr["title"] == format!("chore(release): {tag}")
        && pr["body"] == body
    {
        return report::note(&format!(
            "Release PR is already current: {}",
            pr["html_url"].as_str().context("PR URL")?
        ));
    }
    let pr = if let Some(pr) = prs.first() {
        github.write(
            "PATCH",
            &format!("pulls/{}", pr["number"]),
            json!({"title": format!("chore(release): {tag}"), "body": body}),
        )?
    } else {
        github.write("POST", "pulls", json!({"head": "release/next", "base": "trunk", "title": format!("chore(release): {tag}"), "body": body}))?
    };
    report::note(&format!(
        "Release PR {}: {}",
        if prs.is_empty() {
            "created"
        } else {
            "refreshed"
        },
        pr["html_url"].as_str().context("PR URL")?
    ))
}

fn write_files(root: &Path, base: &str, version: &semver::Version) -> Result<String> {
    ensure!(
        run(Command::new("git")
            .current_dir(root)
            .args(["rev-parse", "HEAD"]))?
            == base,
        "release checkout must match the selected base commit"
    );
    let manifest = fs::read_to_string(root.join("Cargo.toml"))?;
    let expected_lock =
        project::set_lock_versions(&fs::read_to_string(root.join("Cargo.lock"))?, version)?;
    fs::write(
        root.join("Cargo.toml"),
        project::set_version(&manifest, version)?,
    )?;
    // Release preparation changes workspace versions, not dependency resolution or downloaded sources.
    fs::write(root.join("Cargo.lock"), &expected_lock)?;
    run(Command::new("cargo").current_dir(root).args([
        "metadata",
        "--no-deps",
        "--locked",
        "--offline",
        "--format-version",
        "1",
    ]))?;
    let lock = fs::read_to_string(root.join("Cargo.lock"))?;
    project::lock_versions(&lock, version)?;
    ensure!(
        lock.trim_end() == expected_lock.trim_end(),
        "Cargo changed dependency resolution during the release bump"
    );
    let changelog = run(Command::new("convco").current_dir(root).args([
        "changelog",
        "--unreleased",
        &version.to_string(),
        // An explicit SHA becomes Convco's heading; HEAD preserves the --unreleased version label.
        "HEAD",
    ]))?;
    fs::write(
        root.join("CHANGELOG.md"),
        // Changelog sections repeat across versions; other Markdown keeps the repository policy.
        format!(
            "<!-- markdownlint-configure-file {{\"MD024\": {{\"siblings_only\": true}}}} -->\n\n{}\n",
            escape_changelog_html(&changelog)
        ),
    )?;
    run(Command::new("prettier")
        .current_dir(root)
        .args(["--write", "CHANGELOG.md"]))?;
    let changelog = fs::read_to_string(root.join("CHANGELOG.md"))?;
    ensure!(
        changelog
            .lines()
            .find_map(project::heading_version)
            .as_ref()
            == Some(version),
        "generated changelog does not start with the release version"
    );
    Ok(changelog)
}

fn escape_changelog_html(markdown: &str) -> String {
    let mut result = String::with_capacity(markdown.len());
    let mut end = 0;
    // Commit text can resemble HTML; preserve code and links while rendering raw tags literally.
    for (event, range) in pulldown_cmark::Parser::new(markdown).into_offset_iter() {
        if matches!(
            event,
            pulldown_cmark::Event::Html(_) | pulldown_cmark::Event::InlineHtml(_)
        ) {
            result.push_str(&markdown[end..range.start]);
            result.push_str(
                &markdown[range.clone()]
                    .replace('<', "&lt;")
                    .replace('>', "&gt;"),
            );
            end = range.end;
        }
    }
    result.push_str(&markdown[end..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changelog_escapes_html_without_changing_code_or_links() {
        let markdown = "Use <action>@<ref>, `x<y>`, and [docs](https://example.invalid).\n\n<https://example.invalid>\n\n```text\n<action>@<ref>\n```\n\n<div>literal HTML</div>\n";
        let escaped = escape_changelog_html(markdown);
        assert!(escaped.contains("Use &lt;action&gt;@&lt;ref&gt;"));
        assert!(escaped.contains("`x<y>`"));
        assert!(escaped.contains("[docs](https://example.invalid)"));
        assert!(escaped.contains("<https://example.invalid>"));
        assert!(escaped.contains("```text\n<action>@<ref>\n```"));
        assert!(escaped.contains("&lt;div&gt;literal HTML&lt;/div&gt;"));
        assert_eq!(escape_changelog_html(&escaped), escaped);
    }

    #[test]
    fn preparation_does_not_require_cached_dependency_sources() {
        const CHILD: &str = "ACTIONS_RELEASE_TEST_EMPTY_CARGO_HOME";
        if std::env::var_os(CHILD).is_none() {
            let cargo_home = tempfile::tempdir().unwrap();
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "prepare::tests::preparation_does_not_require_cached_dependency_sources",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env("CARGO_HOME", cargo_home.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        for (name, contents) in [
            ("Cargo.toml", include_str!("../../../Cargo.toml")),
            ("Cargo.lock", include_str!("../../../Cargo.lock")),
            (".convco", include_str!("../../../.convco")),
            (
                "crates/actions-release/Cargo.toml",
                include_str!("../Cargo.toml"),
            ),
            (
                "crates/tact/Cargo.toml",
                include_str!("../../tact/Cargo.toml"),
            ),
            (
                "crates/actions-crap/Cargo.toml",
                include_str!("../../actions-crap/Cargo.toml"),
            ),
            ("crates/actions-release/src/main.rs", "fn main() {}\n"),
            ("crates/tact/src/main.rs", "fn main() {}\n"),
            ("crates/actions-crap/src/main.rs", "fn main() {}\n"),
        ] {
            let file = root.join(name);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, contents).unwrap();
        }
        let git = |args: &[&str]| {
            run(Command::new("git")
                .current_dir(root)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .args(args))
            .unwrap()
        };
        git(&["init", "--initial-branch=trunk"]);
        git(&["add", "."]);
        git(&[
            "-c",
            "user.name=Release tests",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-m",
            "feat: initial release fixture",
        ]);
        let base = git(&["rev-parse", "HEAD"]);
        let mut version = project::version(include_str!("../../../Cargo.toml")).unwrap();
        version.patch += 1;
        let expected =
            project::set_lock_versions(include_str!("../../../Cargo.lock"), &version).unwrap();
        let changelog = write_files(root, &base, &version).unwrap();
        assert!(changelog.contains("initial release fixture"));
        assert_eq!(
            fs::read_to_string(root.join("Cargo.lock")).unwrap(),
            expected
        );
        assert_eq!(
            project::version(&fs::read_to_string(root.join("Cargo.toml")).unwrap()).unwrap(),
            version
        );
    }

    #[test]
    fn preparation_updates_both_cargo_versions_and_generates_changelog() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let git = |args: &[&str]| {
            run(Command::new("git")
                .current_dir(root)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .args(args))
            .unwrap()
        };
        git(&["init", "--initial-branch=trunk"]);
        fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = [\"tact\", \"actions-release\"]\nresolver = \"3\"\n[workspace.package]\nversion = \"0.1.0\"\n").unwrap();
        fs::write(root.join(".convco"), include_str!("../../../.convco")).unwrap();
        for name in ["tact", "actions-release"] {
            fs::create_dir_all(root.join(name).join("src")).unwrap();
            fs::write(
                root.join(name).join("Cargo.toml"),
                format!(
                    "[package]\nname = \"{name}\"\nversion.workspace = true\nedition = \"2024\"\n"
                ),
            )
            .unwrap();
            fs::write(root.join(name).join("src/main.rs"), "fn main() {}\n").unwrap();
        }
        run(Command::new("cargo")
            .current_dir(root)
            .args(["generate-lockfile", "--offline"]))
        .unwrap();
        git(&["add", "."]);
        git(&[
            "-c",
            "user.name=Release tests",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-m",
            "feat!: add the initial actions\n\nBREAKING CHANGE: consumers must reference tars-cloud/actions/composite/<action>@<ref>.",
        ]);
        git(&[
            "-c",
            "user.name=Release tests",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "build(deps): add initial dependencies",
        ]);
        let base = git(&["rev-parse", "HEAD"]);
        let output = run(Command::new("convco")
            .current_dir(root)
            .args(["version", "--bump"]))
        .unwrap();
        let version = semver::Version::parse(&output).unwrap();
        let changelog = write_files(root, &base, &version).unwrap();
        assert!(changelog.contains("initial actions"));
        assert_eq!(
            changelog.lines().find_map(project::heading_version),
            Some(version.clone())
        );
        run(Command::new("markdownlint").current_dir(root).args([
            "--disable",
            "MD013",
            "--",
            "CHANGELOG.md",
        ]))
        .unwrap();
        assert_eq!(
            project::version(&fs::read_to_string(root.join("Cargo.toml")).unwrap()).unwrap(),
            version
        );
        project::lock_versions(
            &fs::read_to_string(root.join("Cargo.lock")).unwrap(),
            &version,
        )
        .unwrap();
        git(&["tag", "v0.1.0"]);
        git(&[
            "-c",
            "user.name=Release tests",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "build(deps): update dependencies",
        ]);
        git(&[
            "-c",
            "user.name=Release tests",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "feat!: change the action interface\n\nBREAKING CHANGE: update consumer inputs.",
        ]);
        let base = git(&["rev-parse", "HEAD"]);
        let output = run(Command::new("convco")
            .current_dir(root)
            .args(["version", "--bump"]))
        .unwrap();
        let version = semver::Version::parse(&output).unwrap();
        assert_eq!(version, semver::Version::new(1, 0, 0));
        let changelog = write_files(root, &base, &version).unwrap();
        for heading in ["### Features", "### Dependencies", "### ⚠ BREAKING CHANGE"] {
            assert_eq!(changelog.lines().filter(|line| *line == heading).count(), 2);
        }
        run(Command::new("markdownlint").current_dir(root).args([
            "--disable",
            "MD013",
            "--",
            "CHANGELOG.md",
        ]))
        .unwrap();
        assert_eq!(
            changelog.lines().find_map(project::heading_version),
            Some(version.clone())
        );
        project::lock_versions(
            &fs::read_to_string(root.join("Cargo.lock")).unwrap(),
            &version,
        )
        .unwrap();
        run(Command::new("cargo").current_dir(root).args([
            "metadata",
            "--locked",
            "--offline",
            "--format-version",
            "1",
        ]))
        .unwrap();
        fs::write(
            root.join("CHANGELOG.md"),
            format!("{changelog}\n### Features\n\nDuplicate section in the same version.\n"),
        )
        .unwrap();
        let duplicate = run(Command::new("markdownlint").current_dir(root).args([
            "--disable",
            "MD013",
            "--",
            "CHANGELOG.md",
        ]))
        .unwrap_err();
        assert!(duplicate.to_string().contains("MD024/no-duplicate-heading"));
    }
}
