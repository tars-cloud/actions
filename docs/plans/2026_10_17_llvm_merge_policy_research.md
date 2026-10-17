# LLVM Profile Merge Policy Research

Date: 2026-10-17.
Status: review findings; no runtime implementation.

## Assessment

An opt-in LLVM profile merge policy is feasible and useful for test suites that deliberately terminate subprocesses.
Keep `any` as the default, validate only `any` and `all`, and scope the option to cargo-llvm-cov.
The proposal makes incomplete collection usable; it does not establish complete subprocess coverage.

## Verified Upstream Behaviour

- LLVM documents `any` as the default: one invalid input fails merging.
- LLVM documents `all` as failing only when every input is invalid, excluding invalid input data from successful output.
  [LLVM merge documentation](https://llvm.org/docs/CommandGuide/llvm-profdata.html#cmdoption-llvm-profdata-merge-failure-mode).
- LLVM emits deferred input errors as warnings before deciding whether the selected policy requires failure.
  [LLVM merge implementation](https://github.com/llvm/llvm-project/blob/main/llvm/tools/llvm-profdata/llvm-profdata.cpp).
- cargo-llvm-cov forwards its `--failure-mode` option to llvm-profdata and propagates an unsuccessful merge command.
- No raw inputs and no existing merged profile cause an error; the upstream report command can reuse an existing merged profile, so failure tests need fresh directories.
- Extra `LLVM_PROFDATA_FLAGS` are appended after the selected policy.
  [cargo-llvm-cov report implementation](https://github.com/taiki-e/cargo-llvm-cov/blob/main/src/report.rs).
- Test execution precedes report generation and propagates failures unless the separate `--ignore-run-fail` option is requested.
  [cargo-llvm-cov execution implementation](https://github.com/taiki-e/cargo-llvm-cov/blob/main/src/main.rs).
- The merge option has existed since cargo-llvm-cov 0.1.7, released in September 2021.
  [cargo-llvm-cov changelog](https://github.com/taiki-e/cargo-llvm-cov/blob/main/CHANGELOG.md#017---2021-09-19).

## Repository Integration

- [The analyzer](../../crates/actions-crap/src/main.rs) currently omits the policy, inherits subprocess output, requires successful coverage execution, and rejects missing or structurally empty LCOV before scoring.
- Its effective scoring configuration forces pessimistic missing coverage.
- Its baseline compatibility check compares the entire measurement profile and remeasures the captured baseline source when that profile changes.
- Add the effective policy to that profile and measurement provenance so policy changes cannot reuse a supposedly compatible artifact.
- Old artifacts will become incompatible after adding a profile field; expect a fresh baseline build and additional compute rather than changed comparison semantics.
- [The preflight smoke run](../../composite/cargo-crap/scripts/analyze.sh) also invokes cargo-llvm-cov and should use the selected policy.
- [The composite](../../composite/cargo-crap/action.yaml) and [reusable workflow](../../.github/workflows/consumer-cargo-crap.yaml) both need explicit inputs, environment forwarding, documentation, and contract coverage.

## Caveats and Required Evidence

- Dropping invalid data cannot recover execution counts that were never collected.
- Pessimistic handling remains necessary for functions absent from LCOV, and valid report structure alone cannot prove that all subprocesses were instrumented or collected.
- Inference: discarded inputs can make coverage lower and CRAP scores higher, creating apparent regressions if the amount of discarded data changes between revisions.
  This follows from excluding invalid input data, rather than from a guarantee that the remaining collection represents equivalent execution.
  [LLVM merge documentation](https://llvm.org/docs/CommandGuide/llvm-profdata.html#cmdoption-llvm-profdata-merge-failure-mode).
- Consumer `LLVM_PROFDATA_FLAGS` can contain a contradictory policy or other merge changes, while the current analyzer profile does not record that variable.
  Decide how to reject conflicting policy flags and capture relevant overrides before claiming that recorded policy fully describes execution.
  [Upstream argument ordering](https://github.com/taiki-e/cargo-llvm-cov/blob/main/src/report.rs), [current provenance](../../crates/actions-crap/src/main.rs).
- Keep Tarpaulin invocation unchanged and define its policy metadata as inapplicable, so an ignored LLVM setting does not needlessly invalidate Tarpaulin baselines.
- Add real mixed valid/invalid profile, all-invalid profile, and failing-test cases; mocks alone cannot prove LLVM collection behaviour.
- Use fresh collection directories, preserve diagnostic output, and assert that failures do not emit completed metadata or a passing verdict.
- Exercise the consumer-selected toolchain natively on AMD64 and ARM64 before release.
- The requesting agent’s claimed local profile counts and successful measurements have not been independently reproduced in this review.
