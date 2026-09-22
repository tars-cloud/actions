mod checks;
mod ci;
mod cli;
mod integration;
mod manifest;
mod mock;
mod output;
mod process;
mod runner;

use std::process::ExitCode;

fn main() -> ExitCode {
    if let Some(result) = integration::mock() {
        return match result {
            Ok(code) => ExitCode::from(code),
            Err(error) => {
                eprintln!("tact integration mock: {error:#}");
                ExitCode::from(127)
            }
        };
    }
    if let Some(result) = mock::dispatch() {
        return match result {
            Ok(code) => ExitCode::from(code),
            Err(error) => {
                eprintln!("tact mock: {error:#}");
                ExitCode::from(127)
            }
        };
    }
    match cli::execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("tact: {error:#}");
            ExitCode::FAILURE
        }
    }
}
