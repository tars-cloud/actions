# Coverage Policy and Hosted Cleanup Review

## Recommendation

Proceed with both features, with conservative defaults and the safeguards below.
The LLVM policy is a useful, bounded option for suites that deliberately terminate subprocesses.
Hosted cleanup is useful for reusable jobs because callers cannot insert steps inside them, but automatic SDK removal would change the runtime contract.
Make cleanup opt-in for both workflows and preserve every SDK unless its category is explicitly enabled.

This document records the review that preceded implementation.
The implementation follows the contract below; current usage is documented in the [Cargo CRAP](../../workflows/consumer-cargo-crap/README.md) and [CodeQL](../../workflows/consumer-codeql/README.md) workflow guides.
The reviewed repository revision is `5526d5b27faa17dc4828df7b59e60a287f8d971a`, matching the request's v3.5.0 reference.
The request came from Slopageddon's `ACTIONS-FEATURE-REQUEST.md` on 2026-10-17.
No release or consumer migration is part of this review.

## Evidence and Limits

- [The analyzer](../../crates/actions-crap/src/main.rs) currently runs `cargo llvm-cov --locked` with scope and LCOV output arguments, without a merge policy argument.
  It inherits command output, rejects unsuccessful coverage collection, checks LCOV structure, and validates scoring reports before publishing completed metadata.
- [Baseline selection](../../crates/actions-crap/src/main.rs) compares the complete measurement profile and remeasures the captured baseline revision when an artifact is incompatible.
  The profile already includes an implementation fingerprint, coverage backend/version, compiler identity, scope, declarations, flags, and scoring configuration.
- [LLVM research](2026_10_17_llvm_merge_policy_research.md) verifies upstream policy semantics and identifies environment flags that could override the requested policy.
- [Cargo CRAP](../../.github/workflows/consumer-cargo-crap.yaml) and [CodeQL](../../.github/workflows/consumer-codeql.yaml) currently restore caches and prepare environments without invoking cleanup.
  CodeQL also supports runner mode, C#, C/C++, Java/Kotlin, Go, and macOS Swift.
- [The cleanup action](../../composite/free-disk-space/action.yaml) defaults Android and .NET removal to true.
  [Its scripts](../../composite/free-disk-space/scripts/cleanup.sh) validate Linux X64/ARM64 before skipping self-hosted runners, remove fixed paths only, and report disk usage before and after cleanup.
  Calling it on macOS would fail before the self-hosted guard.
- The GitHub job API confirms that [the cited Slopageddon job](https://github.com/slopageddon/slopageddon/actions/runs/38994191182/job/117282951587) failed on `ubuntu-24.04` during tests and repository checks after cache and environment setup succeeded.
  The reported 56 MB remaining and disk-exhaustion diagnosis were supplied by the requester and were not independently verified from the logs.
  Cleanup cannot guarantee enough capacity for an arbitrary build.

## LLVM Merge Policy

### Proposed LLVM Contract

- Add `llvm-profile-merge-failure-mode` as an optional string input to the [reusable workflow](../../.github/workflows/consumer-cargo-crap.yaml) and [public composite](../../composite/cargo-crap/action.yaml).
  Default to `any`, and reject every value except literal `any` and `all`, including when Tarpaulin is selected.
- Pass the value through a dedicated environment variable and literal argument array.
  For LLVM, append `--failure-mode <value>` to coverage collection for both baseline and current source.
  Keep the smoke test aligned with the chosen policy and validate before running coverage.
- Tarpaulin must receive no new merge argument.
  Document that the input has no effect on its backend.
- Record the effective LLVM policy in `metadata.json` under the compatibility profile.
  Use an explicit non-applicable value for Tarpaulin so changing an unused LLVM input does not invalidate Tarpaulin measurements.
  Do not normalize legacy LLVM metadata without a policy field into a compatible profile.
- Inspect `LLVM_PROFDATA_FLAGS` before coverage collection and reject conflicting merge-policy overrides with a clear error.
  An inherited flag must not make the effective mode disagree with provenance.
  Cover the supported argument spellings in tests rather than relying on command-line ordering.
  Record any remaining supported merge flags in the compatibility profile because they can affect the measurement too.
- Keep stdout and stderr inherited so invalid-profile warnings remain visible.
  Keep collection exit-status checks, LCOV checks, JSON schema validation, source overlap checks, pessimistic missing coverage, strict bootstrap, and absolute threshold enforcement.
  `all` must never imply ignored test failures or complete subprocess instrumentation.

### LLVM Downstream Effects

- Existing callers retain strict LLVM behavior through the `any` default and need no input changes.
  Upstream has supported the option since cargo-llvm-cov 0.1.7; verify the consumer-selected versions in the native fixture matrix before release.
  Never install an undeclared replacement tool.
- New profile fields and the changed implementation fingerprint invalidate saved artifacts from the previous implementation.
  The existing fallback remeasures exact baseline source with the new profile.
  That costs time and disk space and can fail if older source cannot build or test under the selected environment.
  It must not silently enter bootstrap mode or weaken the quality gate because an artifact is incompatible.
- Switching to `all` can produce scores where `any` previously stopped analysis.
  Dropped profiles do not recover lost subprocess coverage; validate the remaining report and disclose the policy.
  Coverage nondeterminism and explicit source exclusions remain separate concerns.
- Rejecting contradictory environment flags can expose a previously hidden consumer configuration conflict.
  Document this error and its remediation rather than silently claiming the input took effect.

## Hosted Cleanup

### Proposed Cleanup Contract

- Add five optional boolean inputs to both reusable workflows: `cleanup-android`, `cleanup-dotnet`, `cleanup-haskell`, `cleanup-boost`, and `cleanup-swift`.
  Default each to `false`.
  Enabling all five provides the complete requested Rust cleanup without introducing an arbitrary path interface.
- Invoke the existing `$/composite/free-disk-space` action only when at least one category is selected and the actual `runner.os` is Linux.
  Pass all five booleans explicitly, including false values, so the primitive's Android/.NET defaults cannot remove unselected SDKs.
  Preserve its platform checks, fixed paths, and self-hosted guard.
- In Cargo CRAP, place cleanup in `analyze` before cache restoration and environment setup.
  In CodeQL, place it in each `analyze` matrix job before cache restoration, environment setup, runner setup commands, and CodeQL initialization.
  Keep it out of comment, baseline-recording, and summary jobs.
- For CodeQL, reject enabled cleanup categories on non-Linux runners with an actionable configuration error before setup.
  Defaults must continue to permit macOS Swift.
  Use actual runner context because matrix rows can override the runner and environment type.
- Initially keep category selection at workflow invocation level.
  Mixed-language callers needing different cleanup choices can use separate workflow invocations.
  Document that every matrix row receives the selected categories; per-row controls can be added later if needed.
- A cleanup failure must fail the analysis job without suppressing final status reporting.
  Include cleanup in Cargo CRAP's aggregate status inputs so a skipped analysis cannot conceal the cause.
  Keep disk reporting visible and do not dump environment variables or credentials.

### Cleanup Downstream Effects

- Defaults preserve the current SDK inventory, CodeQL runner mode, macOS support, and self-hosted behavior.
  Existing callers need no changes.
- Explicitly enabled SDK deletion can break consumer build scripts, native dependencies, or language extractors that require those SDKs.
  Rust projects can also link C/C++ libraries or invoke .NET/Android tools, so selecting `rust` alone does not prove every SDK is disposable.
  Avoid language-based automatic deletion.
- Self-hosted Linux runners retain their files regardless of selected categories.
  Capacity management on persistent runners remains the operator's responsibility.
- The action only reclaims its fixed directories if they exist on the runner image.
  Instrumented builds, restored caches, and fresh baseline measurements can still exhaust disk.
  Do not promise a specific reclaimed size or treat cleanup success as analysis success.
- Keep cleanup out of generic setup-cache and setup-devenv actions.
  [Metadata checks](../../crates/tact/src/checks/metadata.rs) explicitly prohibit implicit cleanup there, and the [cleanup documentation](../../composite/free-disk-space/README.md) must be updated to describe the opt-in workflow callers.

## Implementation and Acceptance

Implement this as two changes so coverage semantics and SDK deletion can be reviewed and reverted independently.
Both fit the existing design and require no new runtime dependencies, permissions, deletion script, or general shell argument passthrough.
The main effort is verification across real coverage collection, reusable workflow wiring, and hosted runner variants.

- Extend [Tact's Cargo CRAP checks](../../crates/tact/src/checks/cargo_crap.rs), action scenarios, and Rust helper tests to verify validation, literal forwarding, smoke coverage, metadata, and baseline incompatibility.
  Verify Tarpaulin does not receive LLVM arguments or acquire policy-dependent profiles.
- Exercise actual compatible LLVM tools with mixed valid/corrupt raw profiles and all-corrupt profiles.
  Keep the corruption fixture deterministic and inject it into the collection used by the analyzer, not an unrelated standalone merge.
  Use fresh collection directories so an existing merged profile cannot conceal a no-input failure.
  Assert warnings, validated scores only for usable collection, and no completed report on failure.
  Separately run a real failing test under both policies.
- Extend [workflow contracts](../../crates/tact/src/checks/workflows.rs) to prove cleanup ordering, explicit category forwarding, default preservation, non-Linux handling, status propagation, and exclusion from reporting jobs.
  Reuse and extend [cleanup scenarios](../../composite/free-disk-space/test.yaml) with mocked deletion commands; never delete actual tooling to test safety.
- Extend [native Cargo CRAP CI](../../.github/workflows/test-cargo-crap.yaml) for both policies on AMD64 and ARM64, retaining direct/flake and separate Tarpaulin coverage.
  Extend [security caller CI](../../.github/workflows/test-security-workflows.yaml) to opt into all categories for Rust environments on both architectures and retain runner-mode compatibility checks.
  Retain nested setup-nix and real lifecycle coverage at the revision under test.
- Add CodeQL C# and macOS/default contract coverage so cleanup cannot regress those callers unnoticed.
  Verify persistent-runner skipping without removing real runner files.
- Update both composite/workflow READMEs, copyable examples, cleanup documentation, and release notes.
  Show explicit Rust opt-in and explain baseline remeasurement and incomplete-profile warnings.
- Run `tact validate`, `tact run`, `tact check metadata`, configured hooks, and `devenv test` through the root development shell.
  Release only after native hosted coverage and security checks pass; local mocks cannot establish ARM64 execution or GitHub composite wiring.

The largest remaining risk is enabling cleanup for a consumer with an undeclared dependency on a hosted SDK.
False defaults and explicit category selection contain that risk.
For the LLVM option, the release blocker is proving that unusable collection and genuine test failures never become completed passing analysis.
