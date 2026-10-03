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
