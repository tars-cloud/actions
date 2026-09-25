# Testing with Tact

Tact means "test actions". It is the repository's Rust CLI for declarative action tests. Every public composite and
internal action has a `test.yaml`. Tact owns scenario execution, repository contracts, integration checks and GitHub
cache fixtures.

## Run the suite

Run these commands from the repository root:

```bash
CI=true SECRETSPEC_PROVIDER=env SECRETSPEC_REASON=tact-review \
  devenv --no-tui shell --quiet -- tact validate
CI=true SECRETSPEC_PROVIDER=env SECRETSPEC_REASON=tact-review \
  devenv --no-tui shell --quiet -- tact list
CI=true SECRETSPEC_PROVIDER=env SECRETSPEC_REASON=tact-review \
  devenv --no-tui shell --quiet -- tact run
CI=true SECRETSPEC_PROVIDER=env SECRETSPEC_REASON=tact-review \
  devenv --no-tui shell --quiet -- tact run composite/setup-nix --case reuse-existing-nix
CI=true SECRETSPEC_PROVIDER=env SECRETSPEC_REASON=tact-review \
  devenv --no-tui test
```

The devenv `tact` command uses `cargo run --locked`. The first run builds the CLI and may download locked Cargo
dependencies. Cargo.lock and devenv.lock pin the dependencies and toolchain inputs. `devenv test` runs Rust tests, every
action scenario, repository metadata checks and the configured hooks. Cargo check, Clippy with warnings denied, and
rustfmt are configured in `devenv.nix`.

## Add an action

1. Create `composite/<action>/` and add `test.yaml` beside `action.yml`, with its schema comment and a unique ID for
   each case.
2. Declare only the source files, real tools and mock commands the cases require.
3. State the expected exit code, outputs and ordered mock calls.
4. Run `tact validate`, `tact run composite/<action>` and `devenv test` through the root devenv shell.

The schema is [schemas/test.schema.json](../schemas/test.schema.json). Tact embeds it in the binary and validates
locally without retrieving remote schemas. See [composite/setup-nix/test.yaml](../composite/setup-nix/test.yaml) for a
compact example. Every manifest starts with `---` and `version: 1`.

- `sources`: repository-relative files or directories copied into each case's temporary workspace.
- `tests`: a nonempty list of cases with unique `id` values and descriptions.
- `command`: an executable name and literal arguments, launched from the temporary workspace.
- `env`: explicit strings added to the child environment.
- `tools`: real executables explicitly exposed from the devenv environment, such as Node or Tact.
- `files`: workspace-relative paths and initial UTF-8 contents.
- `commands`: mock executable names, each with responses matched by exact argument arrays.
- `command-paths`: alternate fixture paths for mocks, used to model profile installation and environment-provided tools.
- `timeout-seconds`: optional case timeout, from 1 to 300 seconds; defaults to 30.
- `expect.exit`: required exit code.
- `expect.calls`: required ordered list of mock calls, including repeats; `[]` expects no mock calls.
- `expect.stdout` and `expect.stderr`: optional exact UTF-8 assertions.
- `expect.github-output` and `expect.github-env`: optional exact maps parsed from GitHub command files, including
  multiline values.
- `expect.github-path`: optional exact list of lines appended to the GitHub path file.
- `expect.files`: optional exact UTF-8 contents; `null` requires a path to be absent.

Unknown fields, duplicate IDs, ambiguous mock responses and reserved environment overrides fail validation. Paths must
be relative and cannot contain traversal; copied sources cannot escape the repository or contain symlinks. Mock
responses can be reused, but the observed call sequence must match `expect.calls` exactly. An unexpected mock invocation
fails the case even if the script swallows its exit status. Calls can also assert `cwd` and selected `env` values. A
response with `forward` executes the argument suffix starting at that index; `prepend-path` models a tool directory
added by shell entry. These options exercise the real inner script through a mocked devenv or Nix dispatcher.
`${workspace}`, `${home}`, `${state}` and `${bin}` expand to fixture paths without invoking a shell.

## Execution and isolation

Every case gets fresh workspace, home, temporary and GitHub command-file paths under `.tars/scratch/tact/`. Those
fixtures are removed after success or failure. Child processes inherit only declared and harness-managed variables. The
fixture PATH supplies Bash and dirname from devenv, declared mocks, and explicitly listed real tools. Mock executables
use the same Rust binary, without generated mock shell scripts. Cases execute sequentially; concurrent mock invocations
within a case are unsupported. Timeouts terminate the scenario's process group.

This is fixture and process isolation, not a security sandbox. Trusted scripts can still use absolute paths or access
the network. Ordinary scenarios perform neither installation nor network access.

`validate` and `list` never execute scenarios. Discovery includes public action folders under `composite/` and private
helpers beneath each action's `scripts/` directory. Selecting a public action also selects its private helpers. Every
discovered action must have a manifest; a missing manifest fails validation. An empty discovery or case selection fails
rather than reporting success. Malformed manifests fail before selected scenarios execute. Failures identify the action
and case, show expected versus actual values, and include captured output. Exit status is 0 on success, 1 on validation
or test failure, and 2 for invalid CLI arguments.

Complex cache contracts use shared Rust modules under `crates/tact/src/checks/`. Manifests select these through
`tact check`; they do not contain a second implementation of cache policy. The cache-plan checks call the production
Node API with a fixed clock and assert results in Rust.

## Integration commands

Run these inside the repository's devenv shell:

```bash
tact integration environments
tact integration environments --direct
tact integration environments --system aarch64-linux
tact integration s3
tact integration cachix
```

Environment checks cover declared and missing Trivy in direct, default-flake and named-flake environments.
The shared flake fixture lives in `tests/fixtures/flakes/`.
Use `--system` to exercise setup and run-devenv with real direct, default-flake and named-flake shells for that system.
Foreign systems require existing runner emulation and Nix `extra-platforms` configuration.
The check verifies both the fixture's selected system and the running Bash architecture.
The public devenv Cachix cache avoids rebuilding the flake task runner under emulation.
S3 checks download the reviewed RunsOn restore/save bundles at pinned revisions and use a disposable localhost denial endpoint with fake credentials.
Cachix checks download the pinned main/post bundle and use mock CLIs for public reads, authenticated reads without uploads, writes, fork isolation and source filtering, including daemon drain.
The S3 and Cachix checks do not contact live cache services.
The Cachix integration uses setup-nix-cache's selection script, and its declarative scenarios cover absent configuration and bootstrap behaviour.

## GitHub lifecycle checks

Expected failures run inside test assertions so successful tests do not emit GitHub error annotations. The reporting
smoke test captures its intentional exit code and labels the generated summary as fixture data. The direct jobs run
`tact integration environments` for both direct and flake missing-Trivy checks on each architecture. Tact captures their
diagnostics and reports a failure only when an assertion fails.

Tact does not interpret composite YAML, GitHub expressions, remote actions or post-job hooks. Setting
`RUNNER_ARCH=ARM64` in a local case checks a platform branch; it does not run on ARM hardware. CI exercises actual
composites on native AMD64/ARM64 runners and the `enterprise/tars-cloud` runner group. Direct and flake jobs exercise
setup-nix-cache with public read-only access, and the direct jobs also verify its unconfigured no-op. The remote
consumer fixture includes setup-nix-cache to check bundled script paths and its nested setup-nix reference at the
revision under test.

Direct jobs override the installed devenv CLI with an installable from the locked flake fixture and execute validation through run-devenv.
Flake jobs exercise run-devenv with explicit native systems, and the nested consumer job invokes it at the revision under test.
The self-hosted job also runs foreign-system integration when the runner advertises support, without configuring emulation.

Cold and warm jobs call `tact ci prepare-cache`, `seed-cache`, `verify-cache` and `verify-hits` around the real cache
action. The warm job depends on the cold job finishing, including upstream post-save hooks. Those jobs build Tact with
`nix-build nix/packages/tact.nix --no-out-link` before entering any devenv shell. The package uses the same locked Nix
inputs and Cargo.lock, and runs the Rust suite during its build. This preserves the check that cache setup does not
require the devenv CLI.

## Live S3 lifecycle

The `s3-cache-cold` and `s3-cache-warm` CI jobs use the public `setup-cache` composite on `enterprise/tars-cloud`. They
consume organization secrets `S3_ENDPOINT`, `S3_BUCKET`, `S3_REGION`, `S3_ACCESS_KEY` and `S3_SECRET_ACCESS_KEY`. Fork
PRs do not run these jobs. The warm job uses the cold job's architecture and waits for its post-save hooks. Both jobs
clear only their run-specific fixture archives before restoration, preventing persistent runner files from satisfying
the evidence checks. Cold checks require misses; warm checks require all six exact hits and the preceding job's evidence
files. The normal self-hosted validation job also uses S3 for its Trivy cache.
