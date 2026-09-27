use anyhow::{Result, ensure};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::process::Command;

pub(super) fn run(root: &Path) -> Result<()> {
    let (system, runner, other_system, other_runner) = match std::env::consts::ARCH {
        "x86_64" => ("x86_64-linux", "X64", "aarch64-linux", "ARM64"),
        "aarch64" => ("aarch64-linux", "ARM64", "x86_64-linux", "X64"),
        _ => anyhow::bail!("unsupported test architecture"),
    };
    let scratch = root.join(".tars/scratch/run-environment");
    fs::create_dir_all(&scratch)?;
    let bash = crate::runner::executable("bash")?;
    for mode in ["devenv", "flakes"] {
        for scenario in [
            "success",
            "exit",
            "pipeline",
            "wrong-shell",
            "missing-platform",
            "startup",
        ] {
            let fixture = tempfile::tempdir_in(&scratch)?;
            let directory = fixture.path();
            let bin = directory.join("bin");
            fs::create_dir(&bin)?;
            for name in ["bash", "dirname"] {
                symlink(crate::runner::executable(name)?, bin.join(name))?;
            }
            for name in [
                "devenv.nix",
                "devenv.yaml",
                "devenv.lock",
                "flake.nix",
                "flake.lock",
            ] {
                fs::write(directory.join(name), "{}")?;
            }
            for name in ["nix", "devenv"] {
                let file = bin.join(name);
                fs::write(
                    &file,
                    format!(
                        "#!{}\n{}",
                        bash.display(),
                        r#"
set -euo pipefail
if [[ $* == 'config show' ]]; then
    printf 'extra-platforms = %s\n' "$PLATFORMS"
    exit 0
fi
if [[ ${0##*/} == devenv ]]; then
    [[ $1 == --no-tui && $2 == --system && $3 == "$ENVIRONMENT_SYSTEM" && $4 == shell && $5 == --quiet && $6 == -- ]]
else
    [[ $1 == develop && $2 == --impure && $3 == --system && $4 == "$ENVIRONMENT_SYSTEM" && $5 == .#named && $6 == --command ]]
fi
shift 6
printf 'entry\n' >> "$ENTRIES"
if [[ $SCENARIO == startup ]]; then exit 126; fi
exec "$@"
"#
                    ),
                )?;
                fs::set_permissions(file, fs::Permissions::from_mode(0o700))?;
            }
            let command = match scenario {
                "exit" => "exit 17",
                "pipeline" => "false | true\nprintf unexpected > result",
                _ => {
                    "printf '%s' \"$VALUE\" > result\nprintf '{\"ok\":true}' > \"$DEVENV_RESULT_FILE\"\nprintf 'live stdout\\n'\nprintf 'live stderr\\n' >&2"
                }
            };
            let wrong_shell = scenario == "wrong-shell";
            let output = Command::new(&bash)
                .arg(root.join("composite/run-devenv/scripts/run.sh"))
                .current_dir(directory)
                .env_clear()
                .env("PATH", &bin)
                .env("GITHUB_WORKSPACE", directory)
                .env("RUNNER_OS", "Linux")
                .env("RUNNER_ENVIRONMENT", "self-hosted")
                // Exercise the foreign-system branch with real host Bash, without QEMU.
                .env(
                    "RUNNER_ARCH",
                    if wrong_shell { runner } else { other_runner },
                )
                .env(
                    "ENVIRONMENT_SYSTEM",
                    if wrong_shell { other_system } else { system },
                )
                .env("ENVIRONMENT_TYPE", mode)
                .env("FLAKE_SHELL", ".#named")
                .env("SCENARIO", scenario)
                .env(
                    "PLATFORMS",
                    if scenario == "missing-platform" {
                        ""
                    } else {
                        "aarch64-linux x86_64-linux"
                    },
                )
                .env("ENTRIES", directory.join("entries"))
                .env("DEVENV_RUN", command)
                .env(
                    "DEVENV_RESULT_FILE",
                    directory.join("structured result.json"),
                )
                .env("VALUE", "literal $(touch injected)\nsecond line")
                .output()?;
            let status = match scenario {
                "success" => 0,
                "exit" => 17,
                "startup" => 126,
                _ => 1,
            };
            ensure!(
                output.status.code() == Some(status),
                "{mode}/{scenario}: {output:?}"
            );
            let entries = fs::read_to_string(directory.join("entries")).unwrap_or_default();
            ensure!(
                entries
                    == if scenario == "missing-platform" {
                        ""
                    } else {
                        "entry\n"
                    },
                "{mode}/{scenario}: expected one shell entry, observed {entries:?}"
            );
            if scenario == "success" {
                ensure!(
                    fs::read_to_string(directory.join("result"))?
                        == "literal $(touch injected)\nsecond line",
                    "literal input changed"
                );
                ensure!(
                    fs::read_to_string(directory.join("structured result.json"))?
                        == "{\"ok\":true}",
                    "structured result lost"
                );
                ensure!(
                    output.stdout == b"live stdout\n" && output.stderr == b"live stderr\n",
                    "logs changed: {output:?}"
                );
            } else {
                ensure!(
                    !directory.join("result").exists(),
                    "{mode}/{scenario}: consumer command ran unexpectedly"
                );
            }
            if matches!(scenario, "wrong-shell" | "missing-platform") {
                ensure!(
                    String::from_utf8_lossy(&output.stdout).contains("::error::"),
                    "missing runner diagnostic: {output:?}"
                );
            }
            ensure!(
                !directory.join("injected").exists(),
                "input executed as shell source"
            );
        }
    }
    Ok(())
}
