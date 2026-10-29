use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::BTreeSet;

use super::transport::Tools;
use crate::config::Package;

pub(super) const RETAINED: &str = "tact-retained-";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Version {
    pub id: u64,
    pub digest: String,
    pub created: u64,
    pub tags: Vec<String>,
}

pub(super) fn endpoint(package: &Package) -> Result<String> {
    let (owner, name) = package
        .destination
        .strip_prefix("ghcr.io/")
        .context("retention requires a GHCR destination")?
        .split_once('/')
        .context("GHCR owner and package name")?;
    Ok(format!(
        "/orgs/{owner}/packages/container/{}/versions",
        name.replace('/', "%2F")
    ))
}

pub(super) fn versions(tools: &mut impl Tools, package: &Package) -> Result<Vec<Version>> {
    let response = tools.run("gh", &[
        "api".into(), format!("{}?per_page=100", endpoint(package)?),
        "--paginate".into(), "--jq".into(),
        "[.[] | {id, digest: .name, created: (.created_at | fromdateiso8601), tags: .metadata.container.tags}]".into(),
    ]).context("list GHCR package versions")?;
    // gh filters each page separately, yielding a stream of JSON arrays.
    let pages = serde_json::Deserializer::from_str(&response)
        .into_iter::<Vec<Version>>()
        .collect::<serde_json::Result<Vec<_>>>()
        .context("parse GHCR package versions")?;
    let versions: Vec<Version> = pages.into_iter().flatten().collect();
    for version in &versions {
        super::digest(&version.digest)?;
    }
    ensure!(
        !versions.is_empty(),
        "GHCR returned no versions after publication"
    );
    Ok(versions)
}

pub(super) fn expired(version: &Version, current: &str, cutoff: u64) -> bool {
    if version.digest == current {
        return false;
    }
    let mut since = version.created;
    for tag in &version.tags {
        let Some(retired) = tag.strip_prefix(RETAINED).and_then(|value| {
            let (time, id) = value.split_once('-')?;
            (id.parse::<u64>().ok()? == version.id).then_some(())?;
            time.parse::<u64>().ok()
        }) else {
            return false;
        };
        since = since.max(retired);
    }
    since < cutoff
}

pub(super) fn prune(
    tools: &mut impl Tools,
    package: &Package,
    current: &str,
    cutoff: u64,
    protected: &BTreeSet<u64>,
) -> Result<()> {
    let versions = versions(tools, package)?;
    for tag in &package.tags {
        ensure!(
            versions
                .iter()
                .any(|version| version.digest == current && version.tags.contains(tag)),
            "GHCR has not confirmed the current {tag} tag; skip pruning"
        );
    }
    for version in versions
        .iter()
        .filter(|version| !protected.contains(&version.id) && expired(version, current, cutoff))
    {
        tools.run(
            "gh",
            &[
                "api".into(),
                "--method".into(),
                "DELETE".into(),
                format!("{}/{}", endpoint(package)?, version.id),
            ],
        )?;
        println!("{}: pruned {}", package.name, version.digest);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_protects_current_recent_retired_and_unrelated_tags() {
        let mut version = Version {
            id: 1,
            digest: "old".into(),
            created: 1,
            tags: vec![],
        };
        assert!(expired(&version, "current", 100));
        assert!(!expired(&version, "old", 100));
        for tag in [
            "2",
            "daily",
            "tact-staging",
            "user-tag",
            "tact-retained-broken",
            "tact-retained-100-1",
            "tact-retained-200-1",
            "tact-retained-99-wrong",
            "tact-retained-99-2",
        ] {
            version.tags = vec![tag.into()];
            assert!(!expired(&version, "current", 100), "{tag}");
        }
        version.tags = vec!["tact-retained-99-1".into()];
        assert!(expired(&version, "current", 100));
        version.tags.push("tact-retained-101-1".into());
        assert!(!expired(&version, "current", 100));
        version.tags.clear();
        version.created = 100;
        assert!(!expired(&version, "current", 100));
    }
}
