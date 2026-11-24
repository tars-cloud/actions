# Cargo CRAP

Compare PR function scores with the actual baseline branch and fail CI on regressions.
Bypassing the failure accepts the debt after merge.
The next completed baseline measurement becomes authoritative for comparisons, including accepted scores above the threshold.
Those scores still fail the absolute threshold gate until reduced.

Copy [example.yaml](example.yaml) into your repository.
The example targets the `v4` release alias; pin a published release's full commit SHA for production.

## Declare the Tools

Cargo, rustc, LLVM tools and `cargo-llvm-cov` must come from the selected project environment.
Rustup is not required; direct Nix-provided Cargo and rustc are supported.
If that environment already provides a stable cargo-crap 0.6.x, 0.7.x or 0.9.x release, the action reuses it.
An existing binary outside those supported release lines fails validation; automatic installation runs only when the binary is absent.
Otherwise, declare it in the project's Cargo.toml and commit the updated Cargo.lock:

```toml
[dev-dependencies]
cargo-crap = "0.9"
```

For a workspace, declare the shared version and inherit it in at least one member:

```toml
# Workspace Cargo.toml
[workspace.dependencies]
cargo-crap = "0.9"
```

```toml
# Member Cargo.toml
[dev-dependencies]
cargo-crap.workspace = true
```

Run `cargo metadata --format-version 1` inside the selected environment to update Cargo.lock, then commit both files.
An unused `[workspace.dependencies]` entry does not resolve or lock the tool.
The action requires a direct dependency of a workspace member; a transitive dependency alone does not authorize installation.
Cargo declarations for automatic installation must resolve to a stable cargo-crap 0.6.x, 0.7.x or 0.9.x release from crates.io.
The action installs the exact version resolved in Cargo.lock and validates its CLI and JSON reports before analysis.
Prereleases and other minor or major release lines require a compatibility review before support is added.
Git/path tools must be provided by the selected environment.

An optional normal dependency is also supported:

```toml
[dependencies]
cargo-crap = { version = "0.9", optional = true }
```

This avoids compiling the tool's library during project tests unless the caller enables its feature.
Preflight runs `cargo metadata --locked --all-features --format-version 1` solely to discover declarations, including optional dependencies.
Coverage still uses the caller's requested features and default features.
It installs the resolved version with explicit `--version`, `--locked`, `--target` and `--root` options.
Installation uses the selected environment's compiler and an action-owned directory under `$RUNNER_TEMP/cargo-crap-helper`.
The project lockfile remains unchanged; `cargo install --locked` uses the published tool package's lockfile for its own dependencies.
See the [Cargo install lockfile contract](https://doc.rust-lang.org/cargo/commands/cargo-install.html#dealing-with-the-lockfile).
There is no global tool installation or ambient runner fallback.

Plain `cargo test` does not export LCOV.
The workflow runs Cargo tests through `cargo-llvm-cov` with LLVM instrumentation.

Declare the compiler and coverage tools in the project's direct devenv module:

```nix
{ pkgs, ... }:
{
  packages = [
    pkgs.cargo-llvm-cov
    pkgs.git
    pkgs.coreutils
  ];
  languages = {
    rust = {
      enable = true;
      toolchainFile = ./rust-toolchain.toml;
    };
  };
}
```

Add `llvm-tools-preview` to the Rust toolchain components and update the project environment lockfile.
For a flake devShell, declare the same packages in `buildInputs` or `packages`, including native `cargo` and `rustc` from the selected toolchain.
Pass `type: flakes` and the selected `flake-shell`, such as `.#ci`.
The current contract supports stable cargo-crap 0.6.x, 0.7.x and 0.9.x releases and requires Rust 1.88 or newer to compile the bundled analysis helper.
The project compiler must also meet the selected cargo-crap release's requirements when installing it.
Cargo-crap 0.7 adds trait default methods to analysis, so upgrading can produce additional findings.
Cargo-crap 0.9 emits workspace-relative report paths; the action accepts these alongside the absolute paths from earlier supported releases.
Artifacts retain checkout-relative paths, and comparisons rebase them into the disposable current checkout so nested workspaces compare across checkouts.
The exact tool version remains part of baseline compatibility; changing it triggers baseline remeasurement.

If the toolchain does not expose its LLVM tools through its sysroot, declare compatible absolute `LLVM_COV` and `LLVM_PROFDATA` values in the environment.
The instrumentation smoke test detects incompatible tools before the test suite.

## Comparison and Reports

`llvm-profile-merge-failure-mode` accepts `any` (default) or `all`.
`any` fails on any unreadable raw profile; `all` warns and merges the remaining valid inputs, failing if none are usable.
This option requires cargo-llvm-cov 0.1.7 or newer in the project environment.
Failing tests, malformed reports, strict bootstrap and the absolute threshold retain their existing behavior.
Missing coverage remains pessimistic; discarded profiles do not establish full subprocess coverage.
The job summary shows the selected LLVM policy.

The effective mode and `LLVM_PROFDATA_FLAGS` are recorded in `metadata.json` under `profile.llvm_profile_merge`.
Changing them invalidates baseline artifact reuse and remeasures the captured baseline source with the new settings.
Older artifacts without that profile field also require remeasurement, which costs time and disk space and can expose older-source build or test failures.
Remove failure-mode options and response files from `LLVM_PROFDATA_FLAGS`; use the workflow input for the merge policy.

PRs must target the designated baseline branch.
An omitted `baseline-branch` selects the repository default branch.
The workflow checks GitHub's proposed merge result against its exact baseline parent, and records both SHAs.
It verifies that the merge has two parents and that its second parent matches the PR event's head SHA.
The first parent supplies the baseline even when the PR event contains an older base SHA.

For merge queues, add the `merge_group` trigger with the `checks_requested` type to the caller workflow as shown in [example.yaml](example.yaml).
The same analysis job and `job-name` report the required check for PRs and merge groups.
Merge-group analysis measures the event's `head_sha` against its exact `base_sha`.
It requires `base_ref` to match the designated baseline branch, `head_sha` to match the workflow SHA and `base_sha` to be an ancestor of `head_sha`, without assuming two merge parents.
Merge groups publish summaries and artifacts but never PR comments or baseline records, even when those options are enabled.

- 🟢 **INFO:** every function is at or below `threshold`, default 30, and no regressions were detected; CI passes.
- 🟠 **WARNING:** a score increased by more than `epsilon`, default 0.01, while every function remains at or below the threshold; CI passes.
- 🔴 **ERROR:** any function is strictly above the threshold; CI fails, including for unchanged or improved existing debt.
- A score of exactly 30 passes the default threshold; a change from 22 to 22.3 is a warning.
- Upstream cargo-crap matches moves and removals.

The workflow reuses a compatible artifact from a trusted baseline-branch run of the same caller workflow when it matches the exact baseline SHA or an ancestor with no Rust, crate-asset, external Cargo dependency, toolchain or `.cargo-crap.toml` changes.
Local workspace member version bumps in `Cargo.toml` and `Cargo.lock` are normalized so release PRs and post-release pushes preserve baseline profile compatibility.
When the measured revision also has no CRAP-relevant changes relative to that compatible artifact, the action reuses its validated reports and LCOV trace without rerunning coverage while still enforcing the quality gate and uploading fresh artifacts.
An expired artifact, inaccessible Actions API, changed profile or relevant source/dependency change without a matching baseline artifact triggers a fresh baseline run once the baseline file on the orphan `crap` branch and the target `Cargo.toml` on the baseline branch exist.
The recorded baseline on the `crap` branch is the bootstrap marker and badge source, and is never substituted for fresh branch evidence.
When the `crap` branch does not yet contain the baseline file for the selected `analysis-id` or the target `Cargo.toml` does not yet exist on the baseline branch, analysis measures only the current revision in bootstrap mode (`baseline_source: bootstrap`) and returns `quality: pass` with `severity: warning` so onboarding PRs and the initial baseline push can merge.
Set `strict-bootstrap: true` to enforce the absolute `threshold` even without that baseline.
It defaults to `false` for compatibility and applies to PRs, merge groups, baseline-branch pushes and manual runs.
Strict bootstrap returns `quality: fail` with `severity: error` for any score strictly above the threshold, including the default 30.
Scores at or below the threshold still return `pass/warning` because baseline comparison is unavailable.
Reports retain `baseline_source: bootstrap` and expose the selected policy as `result.strict_bootstrap`.
Both revisions use the PR's selected environment and effective scoring policy.
The selected compiler is fixed across both runs, and the PR's Cargo configuration replaces historical ancestor configuration in the disposable checkouts.
Changes to that profile invalidate reuse and appear in artifact metadata.
A bootstrapped baseline that cannot compile or test under that profile fails analysis.

The default scope is the full workspace with default features.
Pass `packages: '["my-crate"]'` or `features: '["my-feature"]'` to select explicit scope.
Stable cargo-llvm-cov does not instrument doctests in this default mode.
Nextest, caller-supplied LCOV and SARIF are deferred.

CI preserves deliberate exclusions, allowances and `try-weight` from `.cargo-crap.toml`.
It overrides missing coverage to pessimistic, removes `top` and `min`, disables configured gates and duplicate/AI modes, and uses workflow threshold and epsilon settings.
These changes happen only in disposable analysis checkouts.
Review exclusions and allowances as scoring policy changes.

Every completed analysis writes a summary and uploads reports before the quality check fails.
The PR comment and summary use traffic-light headers and native GitHub alert blocks; the quality step also emits notice, warning or error annotations.
See the [PR comment example](../../composite/cargo-crap/README.md#pr-comment-levels).
The wrapper renders complete upstream reports with CLI gates disabled, then applies an absolute threshold verdict equivalent to `--fail-above`.
Regression deltas remain visible without enabling the blocking `--fail-regression` gate.
Upstream exit code 1 means a completed requested gate failed; exit code 2 means analysis did not complete.
Execution and report validation errors fail immediately and do not publish a completed verdict.
Partial source/LCOV mismatch diagnostics remain in the JSON; zero overlap and empty analysis fail validation.
`post-comment: true` updates one bot-owned comment per `analysis-id` on same-repository PRs.
Fork and Dependabot PRs receive summaries and artifacts without comment writes or App secrets.
Comment publication checks the current PR head and skips superseded results.

Inputs also include `job-name`, JSON `runs-on`, `timeout-minutes`, `system` and `devenv-installable`.
Outputs include `complete`, `quality`, `severity`, `result` JSON, `commit`, `baseline-commit`, `artifact-id`, `records-branch` and `records-commit`.
`quality` remains `pass|fail`; `severity` distinguishes `info`, `warning` and `error`, and `result.above_threshold` counts all current offenders.
Callers must permit `contents: write`, `actions: read` and `pull-requests: write` when recording with `GITHUB_TOKEN`, as shown in the example (or `contents: read` when supplying a GitHub App or setting `publish-records: false`).
GitHub validates optional publisher permissions even when their jobs are disabled; analysis itself keeps read-only credentials.
Execution failures are distinct from completed quality failures.
Use branch protection to require the analysis job, and grant administrators bypass only according to your repository's policy.

## Record the Baseline and Badge

Every baseline branch push, including merges, produces absolute JSON and Shields endpoint JSON using the same coverage run.
`publish-records` defaults to `true` and enables the separate recording job.
Completed baseline-branch measurements remain eligible for recording even when strict bootstrap fails the quality gate.
By default, the recording job authenticates with `GITHUB_TOKEN` (requiring `contents: write` on the caller workflow).
If the `crap` branch is protected, supply an optional repository-installed GitHub App through `client-id` and `app-private-key` secrets with Contents write permission.
The analysis job receives no App token and never writes to the repository.

The publisher commits directly to a dedicated orphan branch named `crap` with no shared history with the default branch, so baseline and badge updates never create PRs or commit noise on the default branch.
The recording job uses the repository-wide `cargo-crap-records-${{ github.repository }}` concurrency group across all caller workflows.
It queues publishers with `queue: max` and `cancel-in-progress: false`, so only one recording job writes at a time.
Keep this group shared across callers that write to `crap`; do not add a workflow name, commit SHA, run ID or analysis ID to it.

Files stored on the `crap` branch:

- When `analysis-id` is `default`: `baseline.json` and `crap-badge.json` at the branch root.
- When `analysis-id` is a custom name: `<analysis-id>/baseline.json` and `<analysis-id>/crap-badge.json`, preserving other `analysis-id` subtrees on the same branch via `base_tree`.

The first recording creates the orphan `crap` branch with zero parents; subsequent updates fast-forward its tip.
Recording commits carry the `Tars-Cloud-CRAP: v2` Git trailer for ownership.
The publisher rejects unowned existing `crap` branches, skips stale baseline measurements, and checks for identical content before writing so unchanged scores create no empty commit.
Manual `workflow_dispatch` runs on the baseline branch remain eligible to publish to `crap`.

Projects choose whether and where to display the badge.
Copy the [README badge example](../../composite/cargo-crap/README.md#readme-badge-example) for a public repository (pointing at `https://raw.githubusercontent.com/<owner>/<repo>/crap/crap-badge.json`).

The badge reflects the latest recorded measurement on `crap`, including accepted debt.
It counts functions whose CRAP score exceeds the configured `threshold`, which defaults to 30.
Lower per-function scores and fewer flagged functions are better.
The badge is green (`brightgreen`) for zero flagged functions, orange for 1–5, and red for 6 or more.
These colours describe the count of flagged functions; the PR quality gate fails on any offender, even when the badge is orange.
Append `&style=flat-square` to the Shields URL for small badges with square corners.
Shields may cache responses.
Private repositories need project-provided hosting that Shields can reach.

Use a unique `analysis-id` per invocation when testing multiple profiles; each profile writes to its own `<analysis-id>/` directory on `crap`.
Native Linux AMD64 and ARM64 runners are supported; projects only need their chosen architecture.

## Local Use

Run coverage on demand inside the project's devenv shell:

```bash
cargo llvm-cov --workspace --locked --lcov --output-path lcov.info
cargo crap --workspace --lcov lcov.info
```

Do not add full coverage runs to commit hooks.
Upstream documentation: [regression gate](https://github.com/minikin/cargo-crap/blob/v0.9.0/docs/guides/regression-gate.md), [badge](https://github.com/minikin/cargo-crap/blob/v0.9.0/docs/guides/badge.md), and [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov).

## Common Project Setup

See [shared setup and runner selection](../../docs/setup.md) for LFS checkout, SecretSpec profiles and optional read-only dependency App authentication.
`runs-on` applies to all jobs, including reporting.
`reporting-runs-on` optionally overrides reporting jobs; empty inherits `runs-on` or the organisation policy.

Runner defaults follow the [organisation runner policy](../../docs/setup.md#runner-selection).
`runner-architecture` accepts `AMD64` (default) or `ARM64`; ARM64 selects `aarch64-linux` unless `system` explicitly overrides it.
Explicit `runs-on` selectors take precedence over organisation defaults.

## Hosted Disk Cleanup

SDK cleanup is enabled by default on GitHub-hosted Linux runners and runs before cache restoration and environment setup in the analysis job only.
Each boolean input defaults to `true` and can be set to `false` individually to retain a needed SDK:

- `cleanup-android`: remove the hosted Android SDK.
- `cleanup-dotnet`: remove the hosted .NET SDK.
- `cleanup-haskell`: remove hosted Haskell tools.
- `cleanup-boost`: remove hosted Boost headers.
- `cleanup-swift`: remove the hosted Swift SDK.

Rust build scripts and native dependencies can still use specific hosted SDKs; set any required category to `false` to keep it.
The workflow reuses [free-disk-space](../../composite/free-disk-space/README.md), including its fixed paths, disk reporting and unconditional self-hosted skip.
Cleanup failures fail the job and appear in its status report.
Cleanup does not run in comment or baseline-recording jobs and cannot guarantee sufficient disk capacity.

## Dependency caches

The selected devenv or flake jobs restore detected language dependency caches before preparing the environment.
GitHub-hosted runners use GitHub cache storage.
Trusted self-hosted runs require S3 configuration; missing configuration fails instead of falling back to GitHub storage.
Set `TARS_CLOUD_S3_ENDPOINT`, `TARS_CLOUD_S3_BUCKET` and `TARS_CLOUD_S3_REGION` as repository or organisation variables (read automatically from `vars.*`).
Pass `TARS_CLOUD_S3_ACCESS_KEY` and `TARS_CLOUD_S3_SECRET_ACCESS_KEY` as workflow secrets, with optional `TARS_CLOUD_S3_SESSION_TOKEN`.
Callers may use `secrets: inherit` when these names are available.
Fork PRs use GitHub storage without receiving S3 credentials.
These archives do not contain the Nix store; configure binary substituters on the runner separately.

## Debug Logging

Pass `debug: true` under the calling job's `with:` to enable verbose tool logs and phase diagnostics.
GitHub's **Re-run with debug logging** enables the same behaviour automatically.
See [the debug contract](../../README.md#debug-logging) for details.
