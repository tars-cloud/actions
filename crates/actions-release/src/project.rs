use anyhow::{Context, Result, ensure};
use semver::Version;
use std::fs;
use std::process::Command;
use toml_edit::{DocumentMut, value};

pub(crate) const RELEASE_FILES: [&str; 3] = ["Cargo.toml", "Cargo.lock", "CHANGELOG.md"];

pub(crate) fn version(manifest: &str) -> Result<Version> {
    let document: DocumentMut = manifest.parse()?;
    let version = Version::parse(
        document["workspace"]["package"]["version"]
            .as_str()
            .context("workspace.package.version")?,
    )?;
    ensure!(
        version.pre.is_empty() && version.build.is_empty(),
        "only stable repository releases are supported"
    );
    Ok(version)
}

pub(crate) fn next_version(revision: &str, config: &str) -> Result<Version> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("convco.yaml");
    fs::write(&path, config)?;
    let output = crate::run(
        Command::new("convco")
            .args(["version", "--bump", "--config"])
            .arg(path)
            .arg(revision),
    )?;
    Version::parse(&output).context("Convco must return one semantic version")
}

pub(crate) fn set_version(manifest: &str, version: &Version) -> Result<String> {
    let mut document: DocumentMut = manifest.parse()?;
    ensure!(
        document["workspace"]["package"]["version"].is_str(),
        "missing workspace version"
    );
    document["workspace"]["package"]["version"] = value(version.to_string());
    Ok(document.to_string())
}

pub(crate) fn lock_versions(lock: &str, expected: &Version) -> Result<()> {
    let document: DocumentMut = lock.parse()?;
    let packages = document["package"]
        .as_array_of_tables()
        .context("lockfile packages")?;
    for name in ["tact", "actions-release"] {
        let package = packages
            .iter()
            .find(|p| p["name"].as_str() == Some(name) && !p.contains_key("source"))
            .with_context(|| format!("missing workspace package {name}"))?;
        ensure!(
            package["version"].as_str() == Some(&expected.to_string()),
            "{name} lockfile version differs from Cargo.toml"
        );
    }
    Ok(())
}

pub(crate) fn set_lock_versions(lock: &str, version: &Version) -> Result<String> {
    let mut document: DocumentMut = lock.parse()?;
    let packages = document["package"]
        .as_array_of_tables_mut()
        .context("lockfile packages")?;
    for name in ["tact", "actions-release"] {
        let package = packages
            .iter_mut()
            .find(|p| p["name"].as_str() == Some(name) && !p.contains_key("source"))
            .with_context(|| format!("missing workspace package {name}"))?;
        package["version"] = value(version.to_string());
    }
    Ok(document.to_string())
}

pub(crate) fn file_at(revision: &str, path: &str) -> Result<String> {
    crate::git(&["show", &format!("{revision}:{path}")])
}

pub(crate) fn full_sha(sha: &str) -> Result<()> {
    ensure!(
        sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()),
        "use the full 40-character commit SHA"
    );
    Ok(())
}

pub(crate) fn release_changes(base: &str, head: &str) -> Result<()> {
    let changed = crate::git(&["diff", "--name-only", base, head])?;
    ensure!(
        !changed.is_empty() && changed.lines().all(|file| RELEASE_FILES.contains(&file)),
        "release PR must change only Cargo.toml, Cargo.lock and CHANGELOG.md"
    );
    Ok(())
}

pub(crate) fn heading_version(line: &str) -> Option<Version> {
    let heading = line
        .strip_prefix('#')?
        .trim_start_matches('#')
        .strip_prefix(' ')?;
    let token = heading.trim_start_matches('[').split([' ', ']']).next()?;
    Version::parse(token.trim_start_matches('v')).ok()
}

pub(crate) fn notes(repo: &str, tag: &str, changelog: &str) -> String {
    let section = changelog
        .lines()
        .skip_while(|line| heading_version(line).is_none())
        .skip(1)
        .take_while(|line| heading_version(line).is_none());
    let mut highlights: Vec<String> = Vec::new();
    for line in section {
        if line.starts_with("* ") || line.starts_with("- ") {
            highlights.push(line.to_owned());
        } else if line.starts_with("  ")
            && let Some(last) = highlights.last_mut()
        {
            last.push('\n');
            last.push_str(line);
        }
    }
    let mut notes = format!("Release {tag} of the TARS Cloud actions and Tact.\n\n");
    if highlights.is_empty() {
        notes.push_str("Repository maintenance and dependency updates.\n\n");
    } else {
        for line in highlights.iter().take(5) {
            notes.push_str(line);
            notes.push('\n');
        }
        if highlights.len() > 5 {
            notes.push_str(&format!(
                "\nPlus {} further changes.\n",
                highlights.len() - 5
            ));
        }
        notes.push('\n');
    }
    notes.push_str(&format!(
        "[Full changelog](https://github.com/{repo}/blob/{tag}/CHANGELOG.md)\n"
    ));
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_edit_preserves_other_configuration() {
        let text = "# repository\n[workspace.package]\nversion = \"0.1.0\"\nedition = \"2024\"\n";
        let next = Version::new(1, 2, 3);
        let edited = set_version(text, &next).unwrap();
        assert_eq!(version(&edited).unwrap(), next);
        assert!(edited.contains("# repository") && edited.contains("edition = \"2024\""));
        assert!(version("[workspace.package]\nversion = \"1.0.0-rc.1\"").is_err());
    }

    #[test]
    fn lockfile_requires_both_workspace_versions() {
        let lock = "[[package]]\nname = \"tact\"\nversion = \"1.2.3\"\n[[package]]\nname = \"actions-release\"\nversion = \"1.2.3\"\n";
        assert!(lock_versions(lock, &Version::new(1, 2, 3)).is_ok());
        assert!(lock_versions(lock, &Version::new(1, 2, 4)).is_err());
        assert!(
            lock_versions(
                "[[package]]\nname = \"tact\"\nversion = \"1.2.3\"",
                &Version::new(1, 2, 3)
            )
            .is_err()
        );
    }

    #[test]
    fn summary_is_bounded_and_links_to_versioned_changelog() {
        let changelog = "# Changelog\n## 1.2.3\n### Features\n* one\n* two\n* three\n* four\n* five\n* six\n## 1.2.2\n* old\n";
        let notes = notes("tars-cloud/actions", "v1.2.3", changelog);
        assert!(notes.contains("* five") && notes.contains("1 further changes"));
        assert!(!notes.contains("* six") && !notes.contains("* old"));
        assert!(notes.contains("/blob/v1.2.3/CHANGELOG.md"));
    }

    #[test]
    fn summary_handles_patch_headings_and_wrapped_bullets() {
        let changelog = "# Changelog\n### [v1.2.3](compare)\n#### Fixes\n- A wrapped\n  description ([commit](url))\n### v1.2.2 (date)\n- old\n";
        let notes = notes("tars-cloud/actions", "v1.2.3", changelog);
        assert!(notes.contains("- A wrapped\n  description ([commit](url))"));
        assert!(!notes.contains("- old"));
    }
}
