mod cache;
pub(crate) mod cargo_crap;
mod consumer_ci;
mod consumer_setup;
mod devenv_update;
mod examples;
mod keys;
mod metadata;
mod releases;
mod result;
mod run_environment;
mod workflows;

pub(crate) use metadata::pinned_upstream;

use anyhow::Result;
use clap::{Subcommand, ValueEnum};
use std::path::Path;

#[derive(Subcommand)]
pub(crate) enum Check {
    /// Verify consumer checkout, dependency authentication and profile selection.
    ConsumerSetup,
    /// Exercise CRAP revision, baseline and publisher policies against a local API fixture.
    CargoCrap,
    /// Verify cache planning against the production Node implementation.
    CachePlan { scenario: CacheScenario },
    /// Verify transport selection and post-save safety.
    CacheAdapter,
    /// Verify optional run-devenv result validation and isolation.
    RunResult { scenario: ResultScenario },
    /// Verify foreign-system validation uses a single consumer shell entry.
    RunEnvironment,
    /// Verify action wiring, pins and CI lifecycle contracts.
    Metadata,
    /// Verify reusable workflow contracts and execute their shell scripts in fixtures.
    Workflows,
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
        Check::ConsumerSetup => consumer_setup::run(root),
        Check::CargoCrap => cargo_crap::github(root),
        Check::CachePlan { scenario } => cache::run(root, scenario),
        Check::CacheAdapter => metadata::adapter(root),
        Check::RunResult { scenario } => result::run(root, scenario),
        Check::RunEnvironment => run_environment::run(root),
        Check::Metadata => metadata::run(root),
        Check::Workflows => workflows::run(root),
    }?;
    println!("Contract checks passed.");
    Ok(())
}
