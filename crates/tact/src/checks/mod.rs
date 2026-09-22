mod cache;
mod keys;
mod metadata;

use anyhow::Result;
use clap::{Subcommand, ValueEnum};
use std::path::Path;

#[derive(Subcommand)]
pub(crate) enum Check {
    /// Verify cache planning against the production Node implementation.
    CachePlan { scenario: CacheScenario },
    /// Verify transport selection and post-save safety.
    CacheAdapter,
    /// Verify action wiring, pins and CI lifecycle contracts.
    Metadata,
}

#[derive(Clone, ValueEnum)]
pub(crate) enum CacheScenario {
    Backend,
    Fork,
    Platform,
    Discovery,
    Selection,
    Paths,
    ContentKeys,
    EnvironmentKeys,
    CompiledKeys,
    TrustScopes,
    ConcurrentKeys,
}

pub(crate) fn run(root: &Path, check: &Check) -> Result<()> {
    match check {
        Check::CachePlan { scenario } => cache::run(root, scenario)?,
        Check::CacheAdapter => metadata::adapter(root)?,
        Check::Metadata => metadata::run(root)?,
    }
    println!("Contract checks passed.");
    Ok(())
}
