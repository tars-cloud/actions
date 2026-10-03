# Consumer Cargo CRAP

Compare PR function scores with the actual baseline branch and fail CI on regressions.
Bypassing the failure accepts the debt after merge.
The next successful baseline measurement becomes authoritative, including accepted scores above the threshold.

Copy [example.yaml](example.yaml) into the consumer repository.
Pin an immutable actions revision when adopting this unreleased interface.
The `v3` reference in the example becomes usable once the change is released.

## Declare the Tools

Project tools must come from the selected consumer environment.
Missing tools fail preflight with declaration instructions; the workflow does not install them independently.
Plain `cargo test` does not export LCOV.
The default backend runs Cargo tests through cargo-llvm-cov with LLVM instrumentation.

Add these declarations to the consumer's direct devenv module, using a pinned actions source:

```nix
{ config, pkgs, ... }:
let
  actionsSource = builtins.fetchTree {
    type = "github";
    owner = "tars-cloud";
    repo = "actions";
    rev = "REPLACE_WITH_REVIEWED_FULL_COMMIT_SHA";
  };
  rustPlatform = pkgs.makeRustPlatform {
    cargo = config.languages.rust.toolchainPackage;
    rustc = config.languages.rust.toolchainPackage;
  };
in
{
  packages = [
    (import "${actionsSource}/nix/packages/cargo-crap.nix" {
      inherit pkgs rustPlatform;
    })
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
The package's source revision, source hash and separately recorded dependency lock are pinned in this repository.

Alternatively declare `pkgs.cargo-tarpaulin` and set `coverage-tool: tarpaulin`.
Tarpaulin always uses `--engine Llvm`; consumer tarpaulin configuration is ignored so it cannot change the scope.
The same Rust LLVM tools are required.
If the toolchain does not expose its LLVM tools through its sysroot, declare compatible absolute `LLVM_COV` and `LLVM_PROFDATA` values in the environment.
The instrumentation smoke test detects incompatible tools before the consumer suite.

## Comparison and Reports

PRs must target the designated baseline branch.
An omitted `baseline-branch` selects the repository default branch; the example uses `trunk`.
The workflow checks GitHub's proposed merge result against its exact baseline parent, and records both SHAs.

- Existing functions fail when their score increases by more than `epsilon`, default 0.01.
- New functions fail strictly above `threshold`, default 30.
- Improvements elsewhere cannot cancel a regression.
- Existing debt passes when unchanged, even above 30.
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
Partial source/LCOV mismatch diagnostics remain in the JSON; zero overlap and empty analysis fail validation.
`post-comment: true` updates one bot-owned comment per `analysis-id` on same-repository PRs.
Fork and Dependabot PRs receive summaries and artifacts without comment writes or App secrets.
Comment publication checks the current PR head and skips superseded results.

Inputs also include `job-name`, JSON `runs-on`, `timeout-minutes`, `system` and `devenv-installable`.
Outputs include `complete`, `quality`, `result` JSON, `commit`, `baseline-commit`, `artifact-id` and `records-pr-url`.
Callers must permit `contents: read`, `actions: read` and `pull-requests: write`, as shown in the example.
GitHub validates optional publisher permissions even when their jobs are disabled; analysis itself keeps read-only credentials.
Execution failures are distinct from completed quality failures.
Use branch protection to require the analysis job, and grant administrators bypass only according to the consumer's policy.

## Record the Baseline and Badge

Every baseline branch push, including merges, produces absolute JSON and Shields endpoint JSON using the same coverage run.
Set `update-records-pr: true` to enable the separate recording job.
Supply a repository-installed GitHub App through `app-id` and `app-private-key` secrets, with Contents and Pull Requests write permissions.
The analysis job receives no App token.

The publisher creates or refreshes one bot-owned `crap/next` PR titled `Update CRAP Baseline and Badge`.
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
For a public repository, a possible Shields URL is:

```text
https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/OWNER/REPO/trunk/.github/badges/crap-badge.json
```

The badge reflects the last merged recording PR, including accepted debt.
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
