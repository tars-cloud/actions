# Cargo CRAP

Measure Rust function complexity and test coverage inside a declared devenv or flake environment.
Use the [reusable workflow](../../workflows/consumer-cargo-crap/README.md) for exact baseline selection, reporting, comments and recording PRs.

- `operation: measure` produces an absolute baseline and Shields JSON while accepting existing debt.
- `operation: compare` requires `baseline-commit` and fails the quality verdict when an existing function increases beyond `epsilon`, or a new function exceeds `threshold`.
- `commit-sha` must match the checkout.
- `baseline-directory` may contain a trusted baseline artifact.
  Incompatible or absent artifacts trigger fresh measurement of `baseline-commit`.
- `type`, `working-directory`, `flake-shell` and `system` select the consumer environment.
- `coverage-tool` selects `llvm-cov` by default or `tarpaulin` with its LLVM engine.
- `packages` and `features` are JSON arrays.
  An empty package list selects the full workspace, and default features remain enabled.

The action checks declared tools, verifies a small instrumented fixture, and runs the consumer tests in disposable source checkouts.
It reuses cargo-crap from the selected environment or installs the exact crates.io version declared and resolved in the consumer's Cargo.lock into an action-owned temporary directory.
Direct Nix Cargo is supported without rustup.
It uses the selected environment for both revisions, including when the baseline predates the coverage dependencies.
The original checkout and its scoring configuration remain unchanged.

Completed analysis exposes `complete: true`, `quality: pass|fail`, `result` JSON and `report-directory`.
A quality failure is an output so callers can publish the reports before failing their check.
Callers must enforce `quality == pass` after publication.
Execution or validation errors fail the action immediately.

The report directory contains normalized `baseline.json`, `crap-badge.json`, compatibility `metadata.json`, `result.json`, `summary.md` and detailed `current-reports/` output.
Upload only these files, rather than the disposable source and target directories alongside them.

See [example.yaml](example.yaml) and the [consumer setup requirements](../../workflows/consumer-cargo-crap/README.md#declare-the-tools).

## README Badge Example

Enable `update-records-pr: true` in the reusable workflow and merge its recording PR to publish `.github/badges/crap-badge.json`.
For a public repository, copy this snippet into your README and replace `OWNER`, `REPO` and `trunk` with your repository and baseline branch:

```markdown
[![CRAP](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FOWNER%2FREPO%2Ftrunk%2F.github%2Fbadges%2Fcrap-badge.json&style=flat-square)](https://github.com/OWNER/REPO/blob/trunk/.github/badges/crap-badge.json)
```

The label shows the configured per-function threshold, such as `CRAP > 30`.
The message shows `passing` or the number of functions above that threshold, such as `18 crappy`.
Lower scores and fewer flagged functions are better.

- Green (`brightgreen`): zero functions above the threshold.
- Orange: 1–5 functions above the threshold.
- Red: 6 or more functions above the threshold.

These ranges describe the count of flagged functions, rather than a single repository score.
The PR quality gate still compares individual function scores against the baseline.
The badge updates after each recording PR is merged, including any accepted debt.
`style=flat-square` gives small square-corner badges; other styles are listed in the [Shields endpoint documentation](https://shields.io/badges/endpoint-badge).
Shields must be able to fetch the JSON without authentication; private repositories need consumer-provided public hosting.
Shields and GitHub's image proxy may cache the badge briefly.
