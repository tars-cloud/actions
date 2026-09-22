use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};

use crate::{checks, ci, integration, manifest, runner};

#[derive(Parser)]
#[command(version, about = "Test actions using declarative scenarios")]
struct Cli {
    /// Repository containing action folders and their test.yaml manifests.
    #[arg(long, global = true, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run integration checks requiring real environments or pinned upstream source.
    Integration {
        #[command(subcommand)]
        suite: integration::Integration,
    },
    /// Prepare and verify fixtures in native GitHub runner jobs.
    Ci {
        #[command(subcommand)]
        task: ci::Task,
    },
    /// Run shared contract checks used by action manifests.
    Check {
        #[command(subcommand)]
        check: checks::Check,
    },
    /// Validate manifests without executing commands.
    Validate { action: Option<String> },
    /// List validated scenarios without executing commands.
    List { action: Option<String> },
    /// Execute all scenarios, or select an action and case.
    Run {
        action: Option<String>,
        #[arg(long)]
        case: Option<String>,
    },
}

pub(crate) fn execute() -> Result<()> {
    let cli = Cli::parse();
    let root = cli.root.canonicalize().context("resolve repository root")?;
    if let Command::Integration { suite } = &cli.command {
        return integration::run(&root, suite);
    }
    if let Command::Ci { task } = &cli.command {
        return ci::run(&root, task);
    }
    if let Command::Check { check } = &cli.command {
        return checks::run(&root, check);
    }
    let action = match &cli.command {
        Command::Check { .. } | Command::Integration { .. } | Command::Ci { .. } => unreachable!(),
        Command::Validate { action } | Command::List { action } | Command::Run { action, .. } => {
            action
        }
    };
    let suites = manifest::discover(&root, action.as_deref())?;
    match cli.command {
        Command::Check { .. } | Command::Integration { .. } | Command::Ci { .. } => unreachable!(),
        Command::Validate { .. } => println!("Validated {} manifest(s).", suites.len()),
        Command::List { .. } => {
            for suite in suites {
                for case in suite.manifest.tests {
                    println!("{}/{}: {}", suite.action, case.id, case.description);
                }
            }
        }
        Command::Run { case: filter, .. } => {
            let mut passed = 0;
            let mut failed = 0;
            for suite in &suites {
                for case in &suite.manifest.tests {
                    if filter.as_ref().is_some_and(|id| id != &case.id) {
                        continue;
                    }
                    let label = format!("{}/{}", suite.action, case.id);
                    match runner::run(&root, &suite.manifest, case) {
                        Ok(()) => {
                            println!("PASS {label}");
                            passed += 1;
                        }
                        Err(error) => {
                            eprintln!("FAIL {label}: {error:#}");
                            failed += 1;
                        }
                    }
                }
            }
            ensure!(passed + failed > 0, "selection matched zero tests");
            println!("{passed} passed; {failed} failed.");
            ensure!(failed == 0, "scenario failures");
        }
    }
    Ok(())
}
