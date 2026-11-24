# Cargo CRAP

Measure Rust function complexity and test coverage inside a declared devenv or flake environment with `cargo-llvm-cov`.
Use the [reusable workflow](../../workflows/cargo-crap/README.md) for exact baseline selection, reporting, PR comments, and automatic orphan `crap` branch recording.

- `operation: measure` produces an absolute baseline and Shields JSON, and fails the quality verdict if any function exceeds `threshold` once a baseline exists on the orphan `crap` branch.
- `operation: compare` requires `baseline-commit`, applies the same absolute threshold, and reports regressions beyond `epsilon` as warnings when all functions remain within the threshold.
- When the `crap` branch has no recorded baseline for the selected `analysis-id` or the baseline commit has no `Cargo.toml`, the action measures the current source in bootstrap mode (`baseline_source: bootstrap`) and returns `quality: pass` with `severity: warning` until the first baseline branch measurement publishes to `crap`.
- `strict-bootstrap: "true"` enforces the absolute threshold during bootstrap for both `measure` and `compare`.
  It defaults to `"false"` for compatibility.
  Scores strictly above `threshold` return `fail/error`; scores at or below it retain `pass/warning` because baseline comparison is unavailable.
  `result.strict_bootstrap` records the selected policy, and `baseline_source` remains `bootstrap`.
- `bootstrapped` accepts `auto` (default), `true`, or `false` to declare whether a recorded baseline already exists on the `crap` branch.
- `commit-sha` must match the checkout.
- `baseline-directory` may contain a trusted exact-commit or compatible ancestor baseline artifact.
  Local workspace version bumps in `Cargo.toml` and `Cargo.lock` preserve profile compatibility, and revisions with no Rust, crate-asset, external dependency, toolchain or `.cargo-crap.toml` changes reuse the validated report without rerunning coverage.
  Incompatible or absent artifacts trigger fresh measurement of `baseline-commit` once the repository is bootstrapped.
- `type`, `working-directory`, `flake-shell` and `system` select the project environment.
  A foreign `system` requires preconfigured QEMU/binfmt and Nix `extra-platforms`; the compiler must match the selected environment.
- `llvm-profile-merge-failure-mode` accepts `any` (default) or `all` for `cargo-llvm-cov`.
  `any` fails if any raw profile is unreadable.
  `all` excludes unreadable profiles with warnings and fails if no usable profiles remain.
  Test failures still fail analysis, and missing coverage remains pessimistic.
- `packages` and `features` are JSON arrays.
  An empty package list selects the full workspace, and default features remain enabled.

The action checks declared tools (`cargo-llvm-cov` and `llvm-tools`), verifies a small instrumented fixture, and runs the project tests in disposable source checkouts.
It reuses `cargo-crap` from the selected environment or installs the exact crates.io version declared and resolved in the project's `Cargo.lock` into an action-owned temporary directory.
Stable `cargo-crap` 0.6.x, 0.7.x and 0.9.x releases are supported; prereleases and other release lines are rejected.
CLI smoke checks and JSON schema validation remain required, and the exact tool version remains part of baseline compatibility.
Direct Nix Cargo is supported without rustup.
It uses the selected environment for both revisions, including when the baseline predates the coverage dependencies.
The original checkout and its scoring configuration remain unchanged.

Completed analysis exposes `complete: true`, `quality: pass|fail`, `severity: info|warning|error`, `result` JSON and `report-directory`.
A quality failure is an output so callers can publish the reports before failing their check.
Callers must enforce `quality == pass` after publication.
Execution or validation errors fail the action immediately.

The report directory contains normalized `baseline.json`, `crap-badge.json`, compatibility `metadata.json`, `result.json`, `summary.md` and detailed `current-reports/` output.
Upload only these files, rather than the disposable source and target directories alongside them.

`metadata.json` records the LLVM merge mode and `LLVM_PROFDATA_FLAGS` in `profile.llvm_profile_merge`.
Changing the LLVM mode or merge flags invalidates baseline reuse and triggers fresh measurement of the exact baseline source.
Artifacts from earlier action versions without the field are also incompatible.
`LLVM_PROFDATA_FLAGS` must not specify a failure mode or response file; select the policy through the input.
Partial profiles cannot recover execution counts from terminated subprocesses or prove complete instrumentation.

See [example.yaml](example.yaml) and the [tool declaration requirements](../../workflows/cargo-crap/README.md#declare-the-tools).

## README Badge Example

`publish-records: true` is enabled by default in the reusable workflow and publishes `crap-badge.json` directly to the orphan `crap` branch (at `crap-badge.json` for the default `analysis-id`, or `<analysis-id>/crap-badge.json` for a named analysis).
For a public repository, copy this snippet into your README and replace `OWNER` and `REPO`:

```markdown
[![CRAP](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FOWNER%2FREPO%2Fcrap%2Fcrap-badge.json&style=flat-square)](https://github.com/OWNER/REPO/blob/crap/crap-badge.json)
```

The label shows the configured per-function threshold, such as `CRAP > 30`.
The message shows `passing` or the number of functions above that threshold, such as `18 crappy`.
Lower scores and fewer flagged functions are better.

- Green (`brightgreen`): zero functions above the threshold.
- Orange: 1–5 functions above the threshold.
- Red: 6 or more functions above the threshold.

These ranges describe the count of flagged functions, rather than a single repository score.
The PR quality gate fails whenever any function exceeds the threshold, including unchanged debt; regressions within the threshold are warnings.
The badge updates after each baseline-branch measurement publishes to `crap`, including any accepted debt.
`style=flat-square` gives small square-corner badges; other styles are listed in the [Shields endpoint documentation](https://shields.io/badges/endpoint-badge).
Shields must be able to fetch the JSON without authentication; private repositories need self-hosted public hosting.
Shields and GitHub's image proxy may cache the badge briefly.

## PR Comment Levels

The same report appears in the job summary and the optional sticky PR comment.
The threshold defaults to 30; a score of exactly 30 is allowed.

- 🟢 **INFO:** all functions are within the threshold and no scores regressed; CI passes.
- 🟠 **WARNING:** scores regressed beyond `epsilon`, but all functions remain within the threshold; CI passes with a warning annotation.
- 🔴 **ERROR:** any function exceeds the threshold, including unchanged, moved or improved functions; CI fails with an error annotation.

For example, a change from 22 to 22.3 produces this header:

### 🟠 Cargo CRAP: WARNING

> [!WARNING]
> Scores regressed, but all functions remain within the threshold.
> Review the changes; CI passes.

Reports list the functions above the threshold and changed scores, with measurement details collapsed below them.
The badge colours describe offender counts; an orange badge with 1–5 offenders still corresponds to an ERROR verdict because those functions exceed the threshold.

## Profile and Dependency Access

`secretspec-profile` optionally selects a profile before environment evaluation.
Empty retains the project configuration.
See [project setup](../../docs/setup.md) for shared checkout and dependency credential handling.
`github-token` supplies dependency read access during current and baseline shell entry.

## Debug Logging

Set `debug: true` to enable verbose tool logs and phase diagnostics.
GitHub's **Re-run with debug logging** enables the same behaviour automatically.
The default is `false`; GitHub debug logging still takes precedence.
See [the debug contract](../../README.md#debug-logging) for details.
