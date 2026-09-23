use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::{git, github::Github, project, run};

pub(crate) fn execute(github: &Github) -> Result<()> {
    ensure!(
        git(&["status", "--porcelain", "--untracked-files=no"])?.is_empty(),
        "tracked checkout must be clean"
    );
    git(&["fetch", "origin", "trunk", "--tags"])?;
    let base = git(&["rev-parse", "HEAD"])?;
    ensure!(
        base == git(&["rev-parse", "origin/trunk"])?,
        "trunk advanced since dispatch; run Prepare release again"
    );
    let version = project::next_version(&base, &fs::read_to_string(".convco")?)?;
    let tag = format!("v{version}");
    let tags = git(&["tag", "--list", &tag])?;
    ensure!(
        tags.is_empty(),
        "{tag} already exists; no releasable changes since the last release"
    );

    let owner = github.repo.split('/').next().context("repository owner")?;
    let prs: Vec<Value> = github.get(&format!(
        "pulls?state=open&base=trunk&head={owner}:release/next"
    ))?;
    ensure!(prs.len() <= 1, "multiple release PRs exist");
    let previous = git(&["ls-remote", "origin", "refs/heads/release/next"])?;
    let previous_sha = previous.split_whitespace().next().unwrap_or("");
    git(&["checkout", "-B", "release/next", &base])?;
    let changelog = write_files(Path::new("."), &base, &version)?;
    git(&["add", "--", "Cargo.toml", "Cargo.lock", "CHANGELOG.md"])?;
    ensure!(
        !git(&["diff", "--cached", "--name-only"])?.is_empty(),
        "release is already prepared; publish the merged release commit"
    );
    git(&[
        "-c",
        "user.name=github-actions[bot]",
        "-c",
        "user.email=41898282+github-actions[bot]@users.noreply.github.com",
        "commit",
        "-m",
        &format!("chore(release): {tag}"),
    ])?;
    project::release_changes(&base, "HEAD")?;
    ensure!(
        base == git(&["ls-remote", "origin", "refs/heads/trunk"])?
            .split_whitespace()
            .next()
            .context("remote trunk")?,
        "trunk advanced during preparation; retry"
    );
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
    let body = format!(
        "{}\nPrepared from `{base}` by Convco.\n\nMerge after review and CI, then run **Publish release** with the full merged trunk commit SHA.\nIf trunk advances before merging, rerun **Prepare release**.\n",
        project::notes(&github.repo, &tag, &changelog).replace(
            &format!("/blob/{tag}/CHANGELOG.md"),
            &format!("/blob/{}/CHANGELOG.md", git(&["rev-parse", "HEAD"])?),
        )
    );
    let pr = if let Some(pr) = prs.first() {
        github.write(
            "PATCH",
            &format!("pulls/{}", pr["number"]),
            json!({"title": format!("chore(release): {tag}"), "body": body}),
        )?
    } else {
        github.write("POST", "pulls", json!({"head": "release/next", "base": "trunk", "title": format!("chore(release): {tag}"), "body": body}))?
    };
    println!("Release PR: {}", pr["html_url"]);
    Ok(())
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
        format!("{}\n", escape_changelog_html(&changelog)),
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
            ("crates/actions-release/src/main.rs", "fn main() {}\n"),
            ("crates/tact/src/main.rs", "fn main() {}\n"),
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
        let base = git(&["rev-parse", "HEAD"]);
        let output = run(Command::new("convco")
            .current_dir(root)
            .args(["version", "--bump"]))
        .unwrap();
        let version = semver::Version::parse(&output).unwrap();
        assert_eq!(version, semver::Version::new(0, 1, 1));
        let changelog = write_files(root, &base, &version).unwrap();
        assert!(changelog.contains("Dependencies"));
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
    }
}
