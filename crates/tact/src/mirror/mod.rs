mod retention;
mod transport;

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};

use crate::config::{Mirrors, Package};
use transport::{System, Tools};

pub(crate) fn run(root: &Path, mirrors: &Mirrors) -> Result<()> {
    ensure!(
        env::var_os("DEVENV_ROOT").is_some(),
        "run tact mirror inside the repository devenv shell"
    );
    let repository = env::var("GITHUB_REPOSITORY")
        .context("set GITHUB_REPOSITORY to the publishing repository")?;
    let prefix = format!("ghcr.io/{}/", repository.to_lowercase());
    ensure!(
        mirrors
            .packages
            .iter()
            .all(|package| package.destination.starts_with(&prefix)),
        "mirror destinations must belong to {prefix}"
    );
    for tool in ["oras", "gh"] {
        crate::runner::executable(tool)
            .with_context(|| format!("{tool} is required; enter the repository devenv shell"))?;
    }
    let scratch = root.join(".tars/scratch/mirror");
    fs::create_dir_all(&scratch)?;
    let mut tools = System {
        scratch: scratch.clone(),
        timeout: mirrors.timeout_seconds,
        registry_config: env::var_os("TACT_ORAS_CONFIG").map(Into::into),
    };
    tools.run("oras", &["version".into()])?;
    tools.run("gh", &["--version".into()])?;
    refresh(&mut tools, mirrors, &scratch)
}

fn refresh(tools: &mut impl Tools, mirrors: &Mirrors, scratch: &Path) -> Result<()> {
    let mut failures = Vec::new();
    for package in &mirrors.packages {
        let work = tempfile::tempdir_in(scratch)?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        match sync(tools, package, work.path(), now, mirrors.retention_days) {
            Ok(()) => println!("{}: refresh complete", package.name),
            Err(error) => {
                eprintln!("{}: {error:#}", package.name);
                failures.push(package.name.as_str());
            }
        }
    }
    ensure!(
        failures.is_empty(),
        "mirror refresh failed: {}",
        failures.join(", ")
    );
    Ok(())
}

fn digest(value: &str) -> Result<&str> {
    ensure!(
        value
            .strip_prefix("sha256:")
            .is_some_and(|hash| hash.len() == 64
                && hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))),
        "invalid OCI sha256 digest"
    );
    Ok(value)
}

fn pinned(source: &str, digest: &str) -> String {
    let name = source.split('@').next().unwrap_or(source);
    let name = name
        .rsplit_once(':')
        .map_or(name, |(repository, _)| repository);
    format!("{name}@{digest}")
}

fn resolve(tools: &mut impl Tools, reference: &str, local: bool) -> Result<String> {
    let mut args = vec!["resolve".into(), reference.into()];
    if local {
        args.push("--oci-layout".into());
    }
    let value = tools.run("oras", &args)?;
    Ok(digest(&value)?.to_owned())
}

fn download(tools: &mut impl Tools, package: &Package, work: &Path) -> Result<(String, String)> {
    for (index, source) in package.sources.iter().enumerate() {
        let local = format!(
            "{}:artifact",
            work.join(format!("source-{index}")).display()
        );
        println!("{}: trying {source}", package.name);
        let attempt = (|| -> Result<String> {
            let expected = resolve(tools, source, false)?;
            tools.run(
                "oras",
                &[
                    "cp".into(),
                    "--to-oci-layout".into(),
                    "--no-tty".into(),
                    pinned(source, &expected),
                    local.clone(),
                ],
            )?;
            ensure!(
                resolve(tools, &local, true)? == expected,
                "downloaded manifest digest mismatch"
            );
            Ok(expected)
        })();
        match attempt {
            Ok(digest) => return Ok((local, digest)),
            Err(error) => eprintln!("{}: source failed: {error:#}", package.name),
        }
    }
    bail!("all sources failed; existing public tags preserved")
}

fn sync(tools: &mut impl Tools, package: &Package, work: &Path, now: u64, days: u64) -> Result<()> {
    let (local, digest) = download(tools, package, work)?;
    let staging = format!("{}:tact-staging", package.destination);
    tools.run(
        "oras",
        &[
            "cp".into(),
            "--from-oci-layout".into(),
            "--no-tty".into(),
            local,
            staging.clone(),
        ],
    )?;
    ensure!(
        resolve(tools, &staging, false)? == digest,
        "published manifest digest mismatch; public tags preserved"
    );
    let versions = retention::versions(tools, package)?;
    let previous: BTreeSet<_> = versions
        .iter()
        .filter(|version| {
            version.digest != digest && version.tags.iter().any(|tag| package.tags.contains(tag))
        })
        .map(|version| (version.digest.as_str(), version.id))
        .collect();
    for (old, id) in &previous {
        tools.run(
            "oras",
            &[
                "tag".into(),
                format!("{}@{old}", package.destination),
                format!("{}{now}-{id}", retention::RETAINED),
            ],
        )?;
    }
    let mut args = vec![
        "tag".into(),
        "--concurrency".into(),
        "1".into(),
        format!("{}@{digest}", package.destination),
    ];
    args.extend(package.tags.iter().cloned());
    tools.run("oras", &args)?;
    for tag in &package.tags {
        ensure!(
            resolve(tools, &format!("{}:{tag}", package.destination), false)? == digest,
            "public tag {tag} digest mismatch; skip pruning"
        );
    }
    println!("{}: published {digest}", package.name);
    let protected: BTreeSet<_> = previous.into_iter().map(|(_, id)| id).collect();
    retention::prune(
        tools,
        package,
        &digest,
        now.saturating_sub(days * 86_400),
        &protected,
    )
}

#[cfg(test)]
mod tests;
