# Clean devenv environments

Direct devenv consumers may enable `clean.enabled` in `devenv.yaml`.
Keep the variables needed by the actions you call in `clean.keep`.
The host `PATH` does not need to be retained.
Tool validation uses devenv's declared package profile when the clean shell removes the runner path marker.
Missing tools still fail validation even when the runner has an installed copy.

## Common workflow variables

- `CI`, `SECRETSPEC_PROVIDER`, `SECRETSPEC_PROFILE`, `CONSUMER_SECRETSPEC_PROFILE` and `SECRETSPEC_REASON` select unattended execution and profile.
- `GITHUB_OUTPUT`, `GITHUB_ENV`, `GITHUB_PATH`, `GITHUB_STEP_SUMMARY`, `GITHUB_WORKSPACE` and `RUNNER_TEMP` provide workflow files and temporary storage.
- `DEVENV_RESULT_FILE` carries structured output from `run-devenv`.
- `CARGO_HOME`, `CARGO_TARGET_DIR` and `TRIVY_CACHE_DIR` retain cache locations when those tools are selected.

## Rust release operations

- Keep `RELEASE_ACTION_ROOT`, `RELEASE_COMMAND`, `RELEASE_COMMIT`, `RELEASE_ARTIFACT_RUN` and `RELEASE_MANIFEST`.
- Keep `GITHUB_ACTIONS`, `GITHUB_REPOSITORY`, `GITHUB_REF`, `GITHUB_EVENT_NAME`, `GITHUB_SHA` and `GITHUB_RUN_ID`.
- Keep `GH_TOKEN` and `GH_READ_TOKEN` for the action's separate write and read credentials.
- Convco comes from the consumer environment; versions without `--version-scheme` use their built-in SemVer behavior.
- Convco's version policy, including pre-1.0 bumps, remains the policy of that declared version and repository configuration.

## CodeQL

- Keep `CODEQL_LANGUAGE` and `CODEQL_EXPORT_VARIABLES`.
- Keep `SETUP_COMMAND` and `BUILD_COMMAND` when using those optional workflow inputs.
- Declare Cargo, rustc and Rust sources for Rust analysis.

## Trivy

- Keep `SCAN_CONFIG`, `GATE_CONFIG`, `GATE_REPORT`, `SCAN_TARGET`, `SARIF_FILE`, `FAIL_ON_FINDINGS` and `TRIVY_GITHUB_TOKEN`.
- Declare Trivy in the selected environment.

Keep any additional variables used by your own `run-devenv` commands explicitly.
For shared consumer CI, keep `LINT_COMMAND`, `TEST_COMMAND` and `CONSUMER_TYPE` when running explicit shell commands.
Do not print or copy authentication variables into result files or cache archives.
