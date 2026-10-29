use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, ensure};

pub(super) trait Tools {
    fn run(&mut self, program: &str, args: &[String]) -> Result<String>;
}

pub(super) struct System {
    pub scratch: PathBuf,
    pub timeout: u64,
    pub registry_config: Option<PathBuf>,
}

pub(super) fn auth_args(args: &mut Vec<String>, config: &Path) {
    let config = config.to_string_lossy().into_owned();
    if args.first().is_some_and(|arg| arg == "cp") {
        args.extend(["--from-registry-config".into(), config.clone()]);
        args.extend(["--to-registry-config".into(), config]);
    } else {
        args.extend(["--registry-config".into(), config]);
    }
}

impl Tools for System {
    fn run(&mut self, program: &str, args: &[String]) -> Result<String> {
        let mut args = args.to_vec();
        if program == "oras"
            && args.first().is_some_and(|arg| arg != "version")
            && let Some(config) = &self.registry_config
        {
            auth_args(&mut args, config);
        }
        let mut command = Command::new(crate::runner::executable(program).with_context(|| {
            format!("{program} is required; enter the repository devenv shell")
        })?);
        command.args(&args).env_remove("GH_DEBUG");
        let result = crate::process::run(&mut command, &self.scratch, self.timeout)?;
        // Registry errors can contain signed URLs. Do not include raw tool diagnostics in CI logs.
        ensure!(
            result.code == Some(0),
            "{program} failed (exit {:?})",
            result.code
        );
        Ok(result.text.trim().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_use_config_files_for_both_copy_endpoints_and_other_operations() {
        for (command, expected) in [
            (
                "cp",
                vec![
                    "cp",
                    "--from-registry-config",
                    "/auth/config.json",
                    "--to-registry-config",
                    "/auth/config.json",
                ],
            ),
            (
                "resolve",
                vec!["resolve", "--registry-config", "/auth/config.json"],
            ),
        ] {
            let mut args = vec![command.to_owned()];
            auth_args(&mut args, Path::new("/auth/config.json"));
            assert_eq!(args, expected);
        }
    }

    #[test]
    fn failed_tools_do_not_expose_diagnostics_and_remove_temporary_logs() -> Result<()> {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.tars/scratch/transport-tests");
        std::fs::create_dir_all(&path)?;
        let scratch = tempfile::tempdir_in(path)?;
        let mut tools = System {
            scratch: scratch.path().into(),
            timeout: 5,
            registry_config: None,
        };
        let error = tools
            .run(
                "bash",
                &["-c".into(), "printf fixture-credential >&2; exit 7".into()],
            )
            .unwrap_err()
            .to_string();
        assert!(error.contains("exit Some(7)"));
        assert!(!error.contains("fixture-credential"));
        assert_eq!(std::fs::read_dir(scratch.path())?.count(), 0);
        Ok(())
    }
}
