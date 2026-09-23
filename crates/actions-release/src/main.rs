mod github;
mod prepare;
mod project;
mod publish;

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use std::process::{Command, ExitCode};

#[derive(Parser)]
#[command(version, about = "Prepare and publish repository releases")]
struct Cli {
    #[command(subcommand)]
    command: Task,
}

#[derive(Subcommand)]
enum Task {
    /// Check local release files and ancestry without contacting GitHub or publishing.
    VerifyCandidate {
        #[arg(long)]
        commit: String,
        #[arg(long)]
        head: String,
    },
    /// Calculate a release from trunk and open or update release/next.
    Prepare,
    /// Publish a release candidate from a successful trunk CI completion event.
    PublishAfterCi,
    /// Publish a merged release commit after its trunk CI succeeds.
    Publish {
        /// Full SHA of the merged release commit on trunk.
        #[arg(long)]
        commit: String,
    },
}

fn run(command: &mut Command) -> Result<String> {
    let program = command.get_program().to_string_lossy().into_owned();
    let output = command
        .output()
        .with_context(|| format!("start {program}"))?;
    ensure!(
        output.status.success(),
        "{program} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim_end().to_owned())
}

fn git(args: &[&str]) -> Result<String> {
    run(Command::new("git").args(args))
}

fn main() -> ExitCode {
    let result = (|| {
        let cli = Cli::parse();
        if let Task::VerifyCandidate { commit, head } = &cli.command {
            project::full_sha(commit)?;
            project::full_sha(head)?;
            println!("{}", publish::verify_candidate(commit, head)?);
            return Ok(());
        }
        let github = github::Github::from_env()?;
        ensure!(
            std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true"),
            "release mutations must run in GitHub Actions"
        );
        ensure!(
            std::env::var("GITHUB_REF").as_deref() == Ok("refs/heads/trunk"),
            "release workflows must run from trunk"
        );
        let expected_event = if matches!(cli.command, Task::PublishAfterCi) {
            "workflow_run"
        } else {
            "workflow_dispatch"
        };
        ensure!(
            std::env::var("GITHUB_EVENT_NAME").as_deref() == Ok(expected_event),
            "expected {expected_event} event"
        );
        match cli.command {
            Task::VerifyCandidate { .. } => unreachable!(),
            Task::Prepare => prepare::execute(&github),
            Task::PublishAfterCi => publish::after_ci(&github),
            Task::Publish { commit } => publish::execute(&github, &commit),
        }
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("actions-release: {error:#}");
            ExitCode::FAILURE
        }
    }
}
