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
        base,
    ]))?;
    fs::write(root.join("CHANGELOG.md"), format!("{changelog}\n"))?;
    run(Command::new("prettier")
        .current_dir(root)
        .args(["--write", "CHANGELOG.md"]))?;
    Ok(fs::read_to_string(root.join("CHANGELOG.md"))?)
}

#[cfg(test)]
mod tests {
    use super::*;

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
            "feat: add the initial actions",
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
