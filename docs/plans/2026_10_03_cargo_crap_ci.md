<!-- omit in toc -->

# Cargo CRAP CI

Date: 2026-10-03
Source: consumer CI design discussion and upstream source research.
Status: implemented and locally verified; native ARM64 and GitHub lifecycle verification remain in repository CI.

- [Context](#context)
- [Decisions Made](#decisions-made)
- [Plan Overview](#plan-overview)
- [Action Items](#action-items)
- [Phase 1: Coverage and Analysis](#phase-1-coverage-and-analysis)
- [Phase 2: Consumer Workflow](#phase-2-consumer-workflow)
- [Phase 3: Comments and Recording PR](#phase-3-comments-and-recording-pr)
- [Phase 4: Verification and Documentation](#phase-4-verification-and-documentation)
- [Risks and Implementation Checks](#risks-and-implementation-checks)
- [References](#references)

_Compare PRs with trunk, fail on regressions, and refresh one PR recording the baseline and badge._

## Context

- Consumers need a reusable cargo-crap job that measures Rust function complexity and test coverage.
- All consumer project commands must run inside the selected direct devenv or flake environment.
- The existing Trivy workflow provides the environment validation pattern, and Rust release preparation provides the managed PR pattern.
- This document records the agreed scope and implementation; creating a skill, publishing a release, and migrating consumers are outside its scope.

## Decisions Made

- 🟢 **Baseline:** The designated baseline branch is always the source of comparison.
  For this design, that branch is `trunk`.
- 🟢 **PR revision:** Compare GitHub's proposed merge result with its exact `trunk` parent.
  Record both full commit SHAs in the report.
- 🟢 **Gate:** Fail when any existing function's score increases beyond the comparison tolerance, or a new function scores above the threshold.
  The initial threshold is 30 and the initial tolerance is 0.01.
  Compare functions individually; an improvement elsewhere cannot cancel a regression.
- 🟢 **Accepted debt:** A bypassed merge accepts the new scores as existing debt.
  The next valid measurement of `trunk` becomes authoritative, even when some functions remain above 30.
- 🟢 **Baseline availability:** Reuse a compatible artifact for the exact baseline commit.
  Compute that baseline automatically in a separate checkout when it is absent, expired, or incompatible.
  Do not substitute a report for an older commit.
- 🟢 **Recorded files:** Committed JSON is a reviewed record.
  Work PR checks continue using fresh `trunk` measurements while the recording PR waits.
- 🟢 **Dependencies:** Fail preflight with setup instructions when declared tools are missing or incompatible.
  Do not use ambient runner project tools or install missing project tools independently.
- 🟢 **Default coverage:** Use cargo-llvm-cov to run Cargo tests with instrumentation and export LCOV.
  Plain `cargo test` alone is insufficient.
- 🟢 **Alternative coverage:** Support cargo-tarpaulin with its LLVM engine.
  Use the same selected backend and coverage settings for both sides of a comparison.
- 🟢 **Reporting:** Always publish a job summary and downloadable reports when analysis completes.
  Offer an optional sticky PR comment from a separate publisher job.
  A score failure remains a failed CI check after reporting.
- 🟢 **Badge:** Generate Shields endpoint JSON from the same coverage run on each push to `trunk`, including merges.
  The badge shows absolute counts of functions above the threshold, including accepted debt.
  Consumers decide how to display it.
- 🟢 **Recording PR:** Create or refresh one managed `crap/next` PR targeting `trunk`.
  Ten work PR merges refresh that same PR rather than creating ten recording PRs.
  A user merges the recording PR to publish the generated files on `trunk`.
- 🟢 **File layout:** Record the baseline at `.github/crap/baseline.json` and the badge at `.github/badges/crap-badge.json`.
  Keep other generated reports in temporary storage and artifacts.
- 🟢 **Compatibility:** Support native Linux AMD64 and ARM64 in direct devenv and flake environments.
  Consumers may select one architecture; repository CI verifies both.
- 🔴 **Rejected:** Publishing directly to a separate `badges` branch is replaced by the managed recording PR design.
- ⚪ **Deferred:** Nextest support, supplied LCOV, manual instrumentation, SARIF upload, duplicate detection, and AI triage are follow-up scope.
  The first version provides the agreed Cargo-test coverage path and tarpaulin alternative.

## Plan Overview

- 🟢 **Phase 1:** `composite/cargo-crap/` owns preflight, coverage, analysis, normalization, and gate results.
  **Priority:** P0.
- 🟢 **Phase 2:** `.github/workflows/consumer-cargo-crap.yaml` owns checkout, baseline resolution, artifacts, and the final CI result.
  **Priority:** P0.
- 🟢 **Phase 3:** Separate publisher jobs update the sticky comment and the single recording PR.
  **Priority:** P1.
- 🟡 **Phase 4:** Local Tact checks and AMD64 integration runs pass; real GitHub caller workflows cover ARM64 and lifecycle verification.
  **Priority:** P1.

## Action Items

**P0 - Implement the analysis contract:**

- [x] Add a reviewed, pinned cargo-crap Nix derivation under `nix/packages/` and declare the local tools in `devenv.nix`.
- [x] Add `composite/cargo-crap/action.yaml`, `test.yaml`, `example.yaml`, `README.md`, and action-owned runtime scripts.
- [x] Implement environment-bound tool checks and a small instrumentation smoke test before consumer tests.
- [x] Implement llvm-cov and tarpaulin coverage adapters with literal argument arrays and isolated output directories.
- [x] Implement report validation, path normalization, baseline compatibility metadata, and regression-plus-new-function gating.
- [x] Add the reusable workflow with exact baseline selection, artifact lookup, fallback measurement, and failure reporting.

**P1 - Implement publishing and verification:**

- [x] Add optional sticky comment publication with stable ownership markers and stale-result protection.
- [x] Add optional recording PR publication with a fixed branch, generated-file allowlist, and GitHub App token.
- [x] Extend Tact's shared Rust checks for workflow interfaces, command execution, report handling, and publishing policy.
- [x] Add native integration callers and disposable GitHub lifecycle tests at the revision under test.
- [x] Add consumer setup documentation, a full copyable caller, and optional local on-demand coverage instructions.
- [x] Run the repository's required checks and resolve failures caused by this change.

Local verification passed the full `devenv test` suite, Tact validation and scenarios, metadata and workflow checks, and configured hooks.
Actual AMD64 integration runs passed for both coverage backends in direct devenv and named flake environments, including unchanged accepted debt.
The pinned Nix package built with its upstream tests enabled.
Native ARM64 execution and GitHub lifecycle results remain CI evidence rather than local claims.

## Phase 1: Coverage and Analysis

### Ownership and Inputs

- Use one public `composite/cargo-crap` action instead of a second public setup-only action.
  Its preflight can be reused by the measure and compare operations.
- Keep runtime scripts and any private helper action inside its own `scripts/` directory.
  Public setup and run actions may be composed, but do not source a sibling action's scripts.
- Proposed inputs are `operation` (`measure` or `compare`), `type`, `working-directory`, `flake-shell`, `system`, `coverage-tool`, `baseline-file`, `threshold`, and `epsilon`.
- Use a full workspace with its default features by default.
  Provide explicit package and feature inputs as literal lists, with matching scope for coverage and analysis.
  Do not infer a narrower scope from changed files.
- `measure` produces an absolute report without rejecting existing debt.
  `compare` requires a verified baseline and evaluates the agreed gate.
- Separate analysis completion from the quality verdict in outputs.
  The reusable workflow must be able to publish completed reports before its final gate fails.
  A tool or input error must still fail immediately and cannot be treated as accepted debt.

### Dependency Checks

- Restrict project-tool lookup to paths supplied by the selected environment, following `setup-trivy`'s PATH boundary and clean-profile handling.
  Restrict lookup during execution as well as during preflight so Cargo cannot discover an ambient subcommand later.
- Check Cargo, rustc, cargo-crap, and the selected coverage backend.
  Check versions, native host architecture, and supported CLI capabilities.
- For llvm-cov, locate matching `llvm-cov` and `llvm-profdata` through the selected Rust toolchain or declared `LLVM_COV` and `LLVM_PROFDATA` values.
  Reject missing or incompatible LLVM tools and disable implicit coverage-tool downloads.
- For tarpaulin, verify the LLVM engine and the toolchain support it requires.
  Do not silently fall back to the x86_64-only ptrace engine.
- Run a tiny bundled Rust coverage smoke fixture inside the selected environment to verify instrumentation, profile merging, LCOV export, and source matching.
  Use temporary directories and the selected compiler rather than runner tools.
- Report all known missing dependencies together with exact declaration examples and lockfile instructions.
  Do not start the consumer test suite when preflight fails.
- The locked Nixpkgs source inspected during planning provides cargo-llvm-cov 0.9.0 and cargo-tarpaulin 0.37.2, but no cargo-crap attribute.
  Provide a package derivation using reviewed source and dependency hashes.
  Document how consumers import that derivation from a pinned actions revision into their environment.
- Verify the supported cargo-crap release and bundled helper MSRV during implementation.
  The inspected cargo-crap 0.6.1 source declares Rust 1.88 for source builds.
  Do not turn that build requirement into an unsupported claim about every prebuilt CLI's consumer compiler requirement.

### Coverage Adapters

These are the core commands run inside the selected consumer environment.
The implementation adds validated paths, scope inputs, and isolated target directories.

```bash
cargo llvm-cov --workspace --locked --lcov --output-path "$LCOV_FILE"
```

```bash
cargo tarpaulin --workspace --locked --engine Llvm --out Lcov --output-dir "$REPORT_DIRECTORY"
```

- Tarpaulin writes `lcov.info` inside its selected output directory.
  Normalize both backends to the same internal LCOV contract.
- Select coverage arguments through arrays rather than shell-source interpolation or `eval`.
- Keep baseline and PR target directories separate and remove stale coverage profiles before measurement.
  Any existing `setup-cache` integration must use distinct architecture, backend, toolchain, and build-variant keys.
  Do not restore raw coverage profiles as current evidence.
- A failing test suite, missing LCOV, malformed LCOV, or failed instrumentation is an execution failure.
  Do not evaluate quality gates against stale reports left by an earlier run.
- The stable default excludes doctest coverage because cargo-llvm-cov requires nightly functionality for it.
  Document the exact default test scope and defer doctest coverage options.

### Configuration and Gate Evaluation

- Generate absolute JSON, delta JSON for comparisons, human-readable Markdown, and Shields JSON for baseline measurements.
  Report rendering may rerun cargo-crap against the same LCOV; it must not rerun consumer tests.
- Validate JSON against the supported tool release's vendored schemas and required fields without fetching schemas at runtime.
  Reject unsupported versions, malformed numbers, and incomplete output.
- Normalize function and diagnostic paths to checkout-relative paths before artifacts, source links, or commits.
  Preserve function identity, scores, line ranges, and nondefault `try_weight`.
- Use file ordering for committed baseline entries and stable JSON serialization.
  Keep commit SHAs, run IDs, and timestamps in artifact metadata and the recording PR body rather than the tracked score JSON.
- Enforce missing coverage as pessimistic and do not let `top`, `min`, or consumer display settings truncate the gate's function set.
  Deliberate source exclusions and function allowances remain explicit configuration and appear in compatibility metadata.
- Inspect the upstream boolean configuration behavior before implementing wrappers.
  Omitting `--fail-above` does not disable `fail-above = true`, and a configured regression gate can interfere with absolute report production.
  Resolve this through a validated effective CI configuration in disposable analysis checkouts, preserving the caller checkout and documenting the overrides.
- Preserve scoring settings such as exclusions and `try-weight`, but keep workflow gate controls, report limits, and duplicate/AI modes under the CI contract.
  Build the effective config with a real TOML parser in a bundled helper and cover it with Tact's Rust checks.
- Use the same effective scoring policy and declared CI toolchain for both revisions.
  If a PR changes the analysis profile, invalidate the saved baseline and rescore the exact `trunk` source using that profile.
  Surface policy changes separately so reviewers can distinguish them from source-score changes.
- Fail existing functions classified as regressed beyond epsilon, including regressions whose final scores remain below 30.
  Fail new functions only when their scores are strictly greater than the configured threshold.
  Preserve upstream move, removal, and function-matching behavior instead of adding a second matcher.
- Treat upstream exit 1 as completed analysis with a gate result, and exit 2 as failed analysis.
  Normalize the quality verdict according to the workflow policy after validating the full reports.
- Reject an unexpected empty analysis or zero source/LCOV overlap for a selected Rust scope.
  Report partial mismatch diagnostics without automatically treating every legitimately uninstrumented function as an instrumentation error.

## Phase 2: Consumer Workflow

### Public Interface

- Add `.github/workflows/consumer-cargo-crap.yaml` with `workflow_call`.
  Keep documentation and `example.yaml` under `workflows/consumer-cargo-crap/`.
- Follow existing environment inputs: `type`, `working-directory`, `flake-shell`, `system`, and `devenv-installable`.
  Add `runs-on`, `job-name`, and `timeout-minutes` using existing conventions.
- Add `baseline-branch`, `coverage-tool`, `threshold`, `epsilon`, `packages`, and `features`.
  Resolve an omitted baseline branch to the consumer's default branch; examples use `trunk`.
- Add `post-comment` and `update-records-pr`, both opt-in.
  Use stable default record paths and automation branch names.
- Expose the aggregate result, quality verdict, analysis completion, compared SHAs, function change counts, report artifact identity, and optional recording PR URL.
- Support PRs targeting the baseline branch, pushes to that branch, and manual refreshes on that branch.
  Validate the event and revision rather than treating other branches as baseline producers.
- Use ordinary `pull_request` events for consumer test execution.
  Do not run PR code under `pull_request_target`.

### PR Execution

- Check out the exact proposed merge commit and verify its baseline parent and PR head against the event.
  Capture the baseline SHA for the run rather than following a moving branch ref mid-analysis.
- Prepare the selected environment and run preflight.
  Make its declared CI tools available to both disposable source checkouts so the first adoption PR can measure an older `trunk` revision lacking the new coverage declarations.
- Look up baseline artifacts only in the consumer repository and designated report-producing workflow.
  Require the exact baseline SHA and a completed valid baseline measurement from a trusted baseline-branch run.
  Do not require unrelated jobs in that caller workflow to have passed if the report-producing job completed successfully.
- Compatibility metadata includes analysis contract version, action revision, architecture, coverage backend and version, compiler/toolchain identity, Cargo test scope, relevant environment flags, and effective scoring settings.
  Treat incomplete or incompatible metadata as a cache miss.
- Download by validated run and artifact IDs to an isolated directory using read-only Actions access.
  Never accept a PR-produced artifact as a published authoritative baseline.
- On a miss, measure the captured baseline commit in a separate checkout using the selected analysis profile.
  If the baseline's tests cannot run under that profile, fail with an execution diagnostic rather than silently weakening the gate.
- Run PR coverage, generate the absolute and delta reports, and evaluate the gate.
  Upload valid reports and write the summary before returning a failed quality result.
- Summaries include the exact compared SHAs, baseline source, analysis scope, improvements, regressions, new functions, and existing above-threshold debt.
  Distinguish a quality failure from a broken analysis and avoid presenting an aggregate score as the gate.
- Fork and Dependabot runs receive no publishing App secrets or private cache credentials.
  They still produce summaries, artifacts, and the quality verdict when dependencies are publicly accessible.

### Baseline Branch Execution

- On each push to `trunk`, check out and measure the exact pushed commit once.
  Generate the absolute baseline report, current-debt summary, and badge JSON from that coverage data.
- Publish a verified baseline artifact after successful analysis even when accepted functions exceed 30.
  Missing dependencies or broken tests must not promote an invalid baseline.
- Include a separate metadata envelope identifying the measured SHA, profile, completion state, and hashes of generated files.
  Upload only the intended generated files, explicitly including the hidden `.github` paths where necessary.
- Let newer commits have independent measurements without allowing old runs to replace the latest managed PR contents.
  The baseline artifact remains tied to its immutable measured commit regardless of publisher eligibility.
- When consumers invoke multiple architecture or backend variants, exactly one configured invocation owns the canonical recording PR and committed badge.
  Other invocations may publish separately named analysis artifacts.

## Phase 3: Comments and Recording PR

### Sticky PR Comment

- Publish after completed PR analysis, including quality failures.
  Skip publication when disabled or when the job lacks appropriate write access.
- Use a separate job with a fresh workspace and no consumer environment entry or consumer code execution.
  Grant only the permissions needed to read the report and update the comment.
- Use a repository-specific marker and verify the bot author before updating an existing comment.
  Include the comparison verdict and links to detailed artifacts.
- Paginate comment lookup and distinguish independent analysis IDs when consumers run more than one profile.
  Reject stale results for a superseded PR head and identify the exact baseline used.
- Keep publication failures separate from the quality verdict.
  Default comment publication errors to a warning so they do not rewrite an otherwise valid comparison.

### Managed Recording PR

- Run only for a completed trusted baseline-branch measurement when `update-records-pr` is enabled.
  This is the optional publisher job in the same reusable workflow.
- Use a repository-scoped GitHub App token with Contents and Pull Requests write permissions, following release preparation's authentication pattern.
  Do not pass that token into consumer environment setup, compilation, or tests.
- Use a fixed `crap/next` branch targeting `trunk`, an ownership marker, and a stable `Update CRAP Baseline and Badge` title.
  Use a Conventional Commit such as `chore(crap): update baseline and badge`.
- Use an action-owned GitHub API publisher to enforce ownership and reject stale branch updates without executing consumer code.
  Build the managed commit on the current baseline tree, retain its previous managed head as a second parent, and require a non-forced reference update.
- Download and validate only the current baseline job's generated JSON artifacts into a clean publisher workspace.
  Allow modifications only to `.github/crap/baseline.json` and `.github/badges/crap-badge.json`.
  Preserve unrelated files and reject an unmanaged branch or PR instead of replacing it.
- Serialize publisher mutations, compare the measured SHA with the current remote `trunk` tip, and let newer runs supersede stale measurements.
  Use guarded branch updates and handle concurrent trunk advancement without rolling back a newer snapshot.
- Refresh the existing open PR and its body as new work PRs merge.
  Create a PR only when the generated contents differ from `trunk`.
  Close an obsolete managed PR when its diff disappears.
- Do not auto-merge the recording PR.
  Human merge publishes the files, and consumers choose their own Shields embed or other viewer.
- Merging a generated-file-only PR must not create an endless sequence of new recording PRs.
  Normalize deterministic reports and treat unchanged generated contents as a no-op.
  Changes in real measured coverage still warrant an updated record.
- The badge remains a record of the last merged recording PR while newer reports wait for review.
  That delay does not affect work PR gating or acceptance of debt already merged into `trunk`.

## Phase 4: Verification and Documentation

### Tact and Rust Checks

- Add a schema-valid `test.yaml` for every new public or private action.
  Keep shared checks in Tact's Rust suite and do not add standalone shell, JavaScript, or Python test runners.
- Check missing tools, ambient-tool rejection, clean environments, tool versions, LLVM mismatches, and unsupported architectures.
- Check both coverage command adapters, literal arguments, matching scopes, path restrictions, isolated profiles, and failing consumer tests.
- Check score increases below 30, new scores above 30, exact threshold boundaries, epsilon boundaries, unchanged debt, improvements, moves, removals, and accepted merges.
- Check malformed JSON/LCOV, empty scope, zero overlap, partial overlap, report normalization, config overrides, and unsupported schema versions.
- Check exact baseline selection, expired/missing artifacts, incompatible profiles, foreign artifact provenance, and automatic fallback.
- Check report preservation on a quality failure and distinguish it from an execution failure.
- Check hidden badge artifact inclusion, comment ownership, stale comments, fork permissions, and failed optional publication.
- Check managed PR ownership, generated-file allowlists, unchanged output, closed/merged PRs, concurrent updates, stale trunk measurements, and baseline-record lag.

### Real GitHub Coverage

- Add `.github/workflows/test-cargo-crap.yaml` with a matrix covering native AMD64 and ARM64, direct devenv and named flake shells, and both coverage backends.
  Use readable job names such as `Cargo CRAP - Devenv - LLVM - ARM64`.
- Add small shared Rust fixtures under `tests/fixtures/cargo-crap/` with real instrumented tests and deliberate score changes.
  Pin their environments and provide both declared-tool and missing-tool cases.
- Consume this repository's actions and reusable workflows at the revision under test.
  Verify nested setup-nix composition as part of the native environment matrix.
- Keep real lifecycle tests for cache post-job saves if cargo caching is enabled.
  Do not replace native architecture evidence with mocked runner labels.
- Use disposable GitHub branches to verify that ten successive source updates refresh one recording PR, that its latest contents are preserved, and that merging it does not create a no-op successor.
  Do not migrate real consumer repositories or use real lab credentials for test scenarios.

### Documentation and Acceptance

- Provide a complete copyable consumer workflow with pushes and PRs targeting `trunk`, optional manual refresh, and explicit permissions.
  Add optional App secrets only for consumers enabling the recording PR job.
- Document the Nix package declarations, LLVM toolchain requirements, backend selection, full-workspace defaults, record lag, and admin bypass behavior.
- Document that the regression gate and absolute-debt badge answer different questions.
  A bypassed high score can leave the badge yellow or red while subsequent unchanged PRs pass.
- Provide local on-demand coverage commands without adding a coverage pre-commit hook.
- Start YAML files with `---`, use `.yaml`, block sequences, stable IDs, step IDs before names, Title Case displays, and job comment headers.
- Update workflow/action metadata checks and same-revision caller checks with the new interfaces.
- Run `tact validate`, `tact run`, and `tact check metadata` through the root devenv shell.
  Run `tact check workflows`, the configured hooks, and `devenv test` through the repository's documented environment commands.
- Accept the implementation only after the native matrix and managed PR lifecycle checks pass.
  Local mocks verify behavior but cannot establish GitHub composite wiring or ARM64 execution.
  The managed PR lifecycle test runs only on a trusted default-branch push and uses disposable branches.

## Risks and Implementation Checks

- 🟡 **Configuration discovery:** Upstream walks parent directories for `.cargo-crap.toml`, lacks a general explicit config-path override, and boolean flags cannot disable configured gates.
  Implement and test isolated effective configuration before claiming local settings cannot alter CI policy.
- 🟡 **Schema drift:** The published schema URLs point at a moving branch, and crawled documentation can differ from the inspected source.
  Pin source and vendor schemas matching the actually supported release.
- 🟡 **First adoption:** `trunk` may predate coverage dependency declarations.
  Use the PR-selected declared CI toolchain for fallback source measurement and test this case explicitly.
- 🟡 **Toolchain changes:** Base source can fail to compile under a changed CI profile, and environment changes can affect coverage independently of source edits.
  Surface profile changes and baseline execution failures rather than inventing comparable scores.
- 🟡 **Conditional code:** AST scoring can include functions absent from a particular feature or platform's compiled test scope.
  Keep scope diagnostics visible and use explicit exclusions where appropriate.
- 🟡 **Nondeterministic coverage:** Flaky tests or runtime-dependent coverage can change scores even with identical source.
  Preserve tolerance and exact report provenance; do not silently raise tolerance or discard regressions.
- 🟡 **Managed PR races:** A source push can occur while an older publisher runs or while a user merges the recording PR.
  Validate current tips, ownership, allowed paths, and guarded updates with real GitHub lifecycle tests.
- 🟡 **Publication configuration:** The App and branch rules must permit creation and refresh of the managed branch.
  Document the required App permissions and report publication failures independently of valid baseline artifacts.
- ⚪ **Deferred extensions:** SARIF is absolute output and cannot be generated using cargo-crap's baseline mode.
  Add it later as a separate optional renderer over the existing LCOV rather than changing the regression contract.

## References

- [Repository instructions](../../AGENTS.md).
- [Trivy tool validation](../../composite/setup-trivy/README.md).
- [Consumer Trivy workflow](../../.github/workflows/consumer-trivy.yaml).
- [Rust release preparation](../../workflows/consumer-rust-release-prepare/README.md).
- [Tact checks and lifecycle testing](../tact.md).
- [Cargo-crap getting started](https://github.com/minikin/cargo-crap/blob/main/docs/getting-started.md).
- [Cargo-crap regression gate](https://github.com/minikin/cargo-crap/blob/main/docs/guides/regression-gate.md).
- [Cargo-crap configuration](https://github.com/minikin/cargo-crap/blob/main/docs/reference/config.md).
- [Cargo-crap JSON format](https://github.com/minikin/cargo-crap/blob/main/docs/reference/json.md).
- [Cargo-crap delta matching source](https://github.com/minikin/cargo-crap/blob/main/src/delta.rs).
- [Cargo-crap CLI configuration and filtering source](https://github.com/minikin/cargo-crap/blob/main/src/main.rs).
- [Cargo-crap exit codes](https://github.com/minikin/cargo-crap/blob/main/docs/reference/exit-codes.md).
- [Cargo-crap PR comment guide](https://github.com/minikin/cargo-crap/blob/main/docs/guides/pr-comment.md).
- [Cargo-crap badge guide](https://github.com/minikin/cargo-crap/blob/main/docs/guides/badge.md).
- [Cargo-llvm-cov usage and LLVM requirements](https://github.com/taiki-e/cargo-llvm-cov).
- [Tarpaulin engines and LCOV options](https://github.com/xd009642/tarpaulin).
- [Tarpaulin LCOV exporter](https://github.com/xd009642/tarpaulin/blob/develop/src/report/lcov.rs).
- [Shields endpoint schema](https://shields.io/badges/endpoint-badge).
- [GitHub workflow run lookup](https://docs.github.com/en/rest/actions/workflow-runs#list-workflow-runs-for-a-repository).
- [Artifact download across runs](https://github.com/actions/download-artifact#download-artifacts-from-other-workflow-runs-or-repositories).
- [Artifact hidden-file handling](https://github.com/actions/upload-artifact#usage).
- [GitHub PR merge revisions](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#how-the-merge-branch-affects-your-workflow).
- [GitHub automation token behavior](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow#triggering-a-workflow-from-a-workflow).
- [Create Pull Request fixed-branch updates](https://github.com/peter-evans/create-pull-request#action-behaviour).
