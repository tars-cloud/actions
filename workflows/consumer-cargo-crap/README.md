# Consumer Cargo CRAP

Compare PR function scores with the actual baseline branch and fail CI on regressions.
Bypassing the failure accepts the debt after merge.
The next completed baseline measurement becomes authoritative for comparisons, including accepted scores above the threshold.
Those scores still fail the absolute threshold gate until reduced.

Copy [example.yaml](example.yaml) into the consumer repository.
Pin an immutable actions revision when adopting this unreleased interface.
The `v3` reference in the example becomes usable once the change is released.

## Declare the Tools

Cargo, rustc, LLVM tools and the coverage backend must come from the selected consumer environment.
Rustup is not required; direct Nix-provided Cargo and rustc are supported.
If that environment already provides cargo-crap 0.6.1, the action reuses it.
An existing binary at another version fails validation; automatic installation runs only when the binary is absent.
Otherwise, declare it in the consumer's Cargo.toml and commit the updated Cargo.lock:

```toml
[dev-dependencies]
cargo-crap = "=0.6.1"
```

For a workspace, declare the shared version and inherit it in at least one member:

```toml
# Workspace Cargo.toml
[workspace.dependencies]
cargo-crap = "=0.6.1"
```

```toml
# Member Cargo.toml
[dev-dependencies]
cargo-crap.workspace = true
```

Run `cargo metadata --format-version 1` inside the selected environment to update Cargo.lock, then commit both files.
An unused `[workspace.dependencies]` entry does not resolve or lock the tool.
The action requires a direct dependency of a workspace member; a transitive dependency alone does not authorize installation.
Cargo declarations for automatic installation must resolve to cargo-crap 0.6.1 from crates.io.
Git/path tools must be provided by the selected environment.

An optional normal dependency is also supported:

```toml
[dependencies]
cargo-crap = { version = "=0.6.1", optional = true }
```

This avoids compiling the tool's library during consumer tests unless the caller enables its feature.
Preflight runs `cargo metadata --locked --all-features --format-version 1` solely to discover declarations, including optional dependencies.
Coverage still uses the caller's requested features and default features.
It installs the resolved version with explicit `--version`, `--locked`, `--target` and `--root` options.
Installation uses the selected environment's compiler and an action-owned directory under `$RUNNER_TEMP/cargo-crap-helper`.
The consumer lockfile remains unchanged; `cargo install --locked` uses the published tool package's lockfile for its own dependencies.
See the [Cargo install lockfile contract](https://doc.rust-lang.org/cargo/commands/cargo-install.html#dealing-with-the-lockfile).
There is no global tool installation or ambient runner fallback.

Plain `cargo test` does not export LCOV.
The default backend runs Cargo tests through cargo-llvm-cov with LLVM instrumentation.

Declare the compiler and coverage tools in the consumer's direct devenv module:

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

Add `llvm-tools-preview` to the Rust toolchain components and update the consumer environment lockfile.
For a flake devShell, declare the same packages in `buildInputs` or `packages`, including native `cargo` and `rustc` from the selected toolchain.
Pass `type: flakes` and the selected `flake-shell`, such as `.#ci`.
The current contract requires cargo-crap 0.6.1 and Rust 1.88 or newer to compile the bundled analysis helper.

Alternatively declare `pkgs.cargo-tarpaulin` and set `coverage-tool: tarpaulin`.
Tarpaulin always uses `--engine Llvm`; consumer tarpaulin configuration is ignored so it cannot change the scope.
The same Rust LLVM tools are required.
If the toolchain does not expose its LLVM tools through its sysroot, declare compatible absolute `LLVM_COV` and `LLVM_PROFDATA` values in the environment.
The instrumentation smoke test detects incompatible tools before the consumer suite.

## Comparison and Reports

PRs must target the designated baseline branch.
An omitted `baseline-branch` selects the repository default branch; the example uses `trunk`.
The workflow checks GitHub's proposed merge result against its exact baseline parent, and records both SHAs.

- 🟢 **INFO:** every function is at or below `threshold`, default 30, and no regressions were detected; CI passes.
- 🟠 **WARNING:** a score increased by more than `epsilon`, default 0.01, while every function remains at or below the threshold; CI passes.
- 🔴 **ERROR:** any function is strictly above the threshold; CI fails, including for unchanged or improved existing debt.
- A score of exactly 30 passes the default threshold; a change from 22 to 22.3 is a warning.
- Upstream cargo-crap matches moves and removals.

The workflow reuses only a compatible artifact for the exact baseline SHA from a trusted baseline-branch run of the same caller workflow.
An expired artifact, inaccessible Actions API, changed profile or missing artifact triggers a fresh baseline run.
Committed `.github/crap/baseline.json` is a reviewed record, and is never substituted for fresh branch evidence.
Both revisions use the PR's selected environment and effective scoring policy.
The selected compiler is fixed across both runs, and the PR's Cargo configuration replaces historical ancestor configuration in the disposable checkouts.
Changes to that profile invalidate reuse and appear in artifact metadata.
A baseline that cannot compile or test under that profile fails analysis.

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
Outputs include `complete`, `quality`, `severity`, `result` JSON, `commit`, `baseline-commit`, `artifact-id` and `records-pr-url`.
`quality` remains `pass|fail`; `severity` distinguishes `info`, `warning` and `error`, and `result.above_threshold` counts all current offenders.
Callers must permit `contents: read`, `actions: read` and `pull-requests: write`, as shown in the example.
GitHub validates optional publisher permissions even when their jobs are disabled; analysis itself keeps read-only credentials.
Execution failures are distinct from completed quality failures.
Use branch protection to require the analysis job, and grant administrators bypass only according to the consumer's policy.

## Record the Baseline and Badge

Every baseline branch push, including merges, produces absolute JSON and Shields endpoint JSON using the same coverage run.
Set `update-records-pr: true` to enable the separate recording job.
Supply a repository-installed GitHub App through `app-id` and `app-private-key` secrets, with Contents and Pull Requests write permissions.
The analysis job receives no App token.

The publisher creates or refreshes one bot-owned `crap/next` PR titled `chore: Update CRAP Baseline and Badge`.
The recording job uses the repository-wide `cargo-crap-records-${{ github.repository }}` concurrency group across all caller workflows.
It queues publishers with `queue: max` and `cancel-in-progress: false`, so only one recording job writes at a time.
Keep this group shared across callers that write `crap/next`; do not add a workflow name, commit SHA, run ID or analysis ID to it.
It changes only:

- `.github/crap/baseline.json`
- `.github/badges/crap-badge.json`

Each new valid measurement refreshes the same open PR.
A user merges it to publish the JSON on the baseline branch.
It is never automatically merged.
The publisher rejects unrelated files and unowned branches or PRs, skips stale branch measurements, and rejects concurrent branch updates through fast-forward ancestry checks.
No score change means no new PR; an obsolete open recording PR is closed.
Provenance stays in artifact metadata and the PR body so merging generated records does not cause a new PR solely for a timestamp or commit SHA.

Consumers choose whether and where to display the badge.
Copy the [README badge example](../../composite/cargo-crap/README.md#readme-badge-example) for a public repository.

The badge reflects the last merged recording PR, including accepted debt.
It counts functions whose CRAP score exceeds the configured `threshold`, which defaults to 30.
Lower per-function scores and fewer flagged functions are better.
The badge is green (`brightgreen`) for zero flagged functions, orange for 1–5, and red for 6 or more.
These colours describe the count of flagged functions; the PR quality gate fails on any offender, even when the badge is orange.
Append `&style=flat-square` to the Shields URL for small badges with square corners.
Reports remain fresh while that PR waits.
Shields may cache responses.
Private repositories need consumer-provided hosting that Shields can reach.

Use a unique `analysis-id` per invocation when testing multiple profiles.
Enable `update-records-pr` on exactly one canonical invocation.
Native Linux AMD64 and ARM64 runners are supported; consumers only need their chosen architecture.

## Local Use

Run coverage on demand inside the consumer's devenv shell:

```bash
cargo llvm-cov --workspace --locked --lcov --output-path lcov.info
cargo crap --workspace --lcov lcov.info
```

Do not add full coverage runs to commit hooks.
Upstream documentation: [regression gate](https://github.com/minikin/cargo-crap/blob/v0.6.1/docs/guides/regression-gate.md), [badge](https://github.com/minikin/cargo-crap/blob/v0.6.1/docs/guides/badge.md), and [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov).
