mod cache;
mod examples;
mod keys;
mod metadata;
mod result;

use anyhow::Result;
use clap::{Subcommand, ValueEnum};
use std::path::Path;

#[derive(Subcommand)]
pub(crate) enum Check {
    /// Verify cache planning against the production Node implementation.
    CachePlan { scenario: CacheScenario },
    /// Verify transport selection and post-save safety.
    CacheAdapter,
    /// Verify optional run-devenv result validation and isolation.
    RunResult { scenario: ResultScenario },
    /// Verify action wiring, pins and CI lifecycle contracts.
    Metadata,
}

#[derive(Clone, ValueEnum)]
pub(crate) enum ResultScenario {
    Success,
    Invalid,
    Isolation,
    Failure,
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
    NixCompiledKeys,
    TrustScopes,
    ConcurrentKeys,
}

pub(crate) fn run(root: &Path, check: &Check) -> Result<()> {
    match check {
        Check::CachePlan { scenario } => cache::run(root, scenario)?,
        Check::CacheAdapter => metadata::adapter(root)?,
        Check::RunResult { scenario } => result::run(root, scenario)?,
        Check::Metadata => metadata::run(root)?,
    }
    println!("Contract checks passed.");
    Ok(())
}
