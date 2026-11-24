# Project Setup and Runners

These interfaces target `v4` releases.
Pin a published `v4` full commit SHA when adopting them; see the [migration guide](migration.md) when upgrading from `v3`.

## Runner Selection

Every reusable workflow reads the caller repository's organisation or repository Actions variables.
Configure these under Settings, Secrets and variables, Actions, Variables.
These settings are variables, not secrets.

- `TARS_CLOUD_RUNNER_TYPE`: unset, empty or `saas` selects GitHub-hosted runners; `self-hosted` selects self-hosted runners.
  Any other value fails runner configuration validation before project setup.
  Values are case-sensitive.
- `TARS_CLOUD_RUNNER_AMD64`: optional JSON selector for AMD64 jobs when the type is `self-hosted`.
  Empty requests the native labels `self-hosted`, `linux`, `x64`.
- `TARS_CLOUD_RUNNER_ARM64`: optional JSON selector for ARM64 jobs when the type is `self-hosted`.
  Empty requests the native labels `self-hosted`, `linux`, `ARM64`.
  It never inherits the AMD64 selector.

The SaaS defaults are `ubuntu-24.04` for AMD64 and `ubuntu-24.04-arm` for native ARM64.
SaaS mode ignores the two self-hosted selector variables.
CodeQL retains hosted macOS for Swift; its unconfigured self-hosted defaults use macOS labels for Swift.
Configure an appropriate macOS selector explicitly when organisation selectors target Linux machines.
Linux environment modes keep their existing platform requirements.
Dependabot PR checks use the same runner policy as other checks.
GitHub controls the separate Dependabot update jobs; the public-repository restriction applies to those update jobs.

Every reusable workflow accepts `runner-architecture`, either `AMD64` (default) or `ARM64`.
For example, a caller can test both architectures while inheriting the organisation policy:

```yaml
---
jobs:
  # -------------------------------------------------
  # Devenv CI
  # -------------------------------------------------
  ci:
    name: CI - ${{ matrix.architecture }}
    strategy:
      fail-fast: false
      matrix:
        architecture:
          - AMD64
          - ARM64
    uses: tars-cloud/actions/.github/workflows/devenv-ci.yaml@v4 # Pin a published release SHA.
    with:
      runner-architecture: ${{ matrix.architecture }}
```

Every reusable workflow also accepts `runs-on` as a JSON runner label, label array or group/labels object.
An explicit selector overrides the organisation defaults for all jobs, including comments, summaries and publishers.
This permits per-workflow exceptions.
Composite actions inherit their caller job's runner; they cannot select it after the job starts.
Custom jobs using composites must implement the policy in their own `runs-on` expression.

```yaml
---
with:
  runs-on: '{"group":"enterprise/tars-cloud","labels":["self-hosted","linux","x64"]}'
```

Cargo CRAP and CodeQL also accept `reporting-runs-on` to override their reporting jobs.
Empty inherits `runs-on` or the organisation policy; explicit `"ubuntu-24.04"` selects hosted reporting alongside self-hosted analysis.
CodeQL matrix `runner` values override individual analysis jobs only.
When using matrix-only runner selection, set `runs-on` or `reporting-runs-on` for the summary job too if the organisation policy does not select its desired runner.
An unavailable self-hosted runner queues the job; there is no automatic switch to hosted infrastructure.
The selected runner group must permit the caller repository and workflow.

Repository-only workflows use the same three `TARS_CLOUD_RUNNER_*` variables.
Dedicated self-hosted integration jobs run only when `TARS_CLOUD_RUNNER_TYPE` is `self-hosted` and their existing credential trust conditions pass.

### ARM64 Emulation on AMD64

For AMD64 NixOS runners in the `enterprise/tars-cloud` group with QEMU/binfmt already configured, set:

- `TARS_CLOUD_RUNNER_TYPE`: `self-hosted`.
- `TARS_CLOUD_RUNNER_AMD64`: `{"group":"enterprise/tars-cloud","labels":["linux","x64"]}`.
- `TARS_CLOUD_RUNNER_ARM64`: `{"group":"enterprise/tars-cloud","labels":["linux","x64"]}`.

The two selectors deliberately target the same physical architecture.
The group must contain machines with the requested labels and permit the repository and workflow.
If only some AMD64 runners support emulation, add a capability label such as `arm64-emulation` to those runners and include it in the ARM64 selector.
Do not label an AMD64 runner as native ARM64 merely because it supports emulation.

Emulation also affects environment builds, including packages compiled before the first project command runs.
Set the reusable workflow's `timeout-minutes` input to include that setup time as well as tests or analysis.
This repository allows 180 minutes for its full-environment and Cargo CRAP tests on all runners after observing a cold environment setup take 101 minutes.
Selecting self-hosted runners does not change reusable workflow timeout defaults.

`runner-architecture: ARM64` selects `aarch64-linux` for the project environment unless the caller supplies `system` explicitly.
The actions require working binfmt execution and Nix `extra-platforms` containing `aarch64-linux`.
They validate foreign execution and do not install QEMU or change the host configuration.
Selecting an AMD64 runner alone does not make arbitrary job steps run as ARM64.
Steps outside the selected environment still execute on the runner's native architecture.

To return to native ARM64, unset `TARS_CLOUD_RUNNER_ARM64` while keeping the self-hosted type.
Jobs then wait for a matching native ARM64 runner rather than choosing AMD64 or SaaS.
To use hosted defaults for both architectures, set only `TARS_CLOUD_RUNNER_TYPE` to `saas`, or leave all three variables unset.

## Checkout and Profile

All reusable workflows accept `lfs` and `secretspec-profile`.
LFS defaults to `false`; an empty profile retains repository configuration.
Existing workflow submodule and exact-revision checkout behavior is preserved.
The [setup-project composite](../composite/setup-project/README.md) exposes checkout depth, reference and submodule options directly.

SecretSpec uses the noninteractive `env` provider.
Keep `SECRETSPEC_PROFILE` and the documented action-specific variables in `devenv.yaml` when enabling a clean shell.
Repositories own their SecretSpec manifests and application secret provisioning.

## Hosted Disk Cleanup

All five Linux environment reusable workflows and [free-disk-space](../composite/free-disk-space/README.md) enable hosted SDK cleanup by default.
Each SDK category (`cleanup-android`, `cleanup-dotnet`, `cleanup-haskell`, `cleanup-boost` and `cleanup-swift`) remains a separate boolean defaulting to `true` so callers can opt out individually by setting a specific flag to `false`.
Cleanup is always skipped on self-hosted runners.

## Private Dependencies

Existing `dependency-token` secrets remain supported.
Alternatively name dependency repositories explicitly and pass dedicated dependency App secrets:

```yaml
---
with:
  dependency-owner: my-organization
  dependency-repositories: '["private-nix-input", "shared-crate"]'
secrets:
  dependency-client-id: ${{ secrets.DEPENDENCY_APP_CLIENT_ID }}
  dependency-app-private-key: ${{ secrets.DEPENDENCY_APP_PRIVATE_KEY }}
```

The App must be installed on the named repositories and permit Contents read access.
Only explicitly named repositories enter the token scope.
Checkout uses the repository token unless the dependency scope explicitly includes the caller repository under the same owner.
Include it when private LFS or submodules need the App credential.
An empty dependency list with App credentials fails before checkout rather than granting organization-wide access.
The minted token is read-only even when the App installation permits writes.
It is separate from `client-id` and `app-private-key` publication credentials.
Tokens are revoked after the job and are not saved in Git configuration or cache archives.
Fork PRs, Dependabot runs and `pull_request_target` do not receive supplied dependency credentials.

The dependency token is passed to Nix environment setup and subsequent shell entries.
For package registries and application secrets, retain the repository's own configuration.

## Shared CI Caches

The [Devenv CI workflow](../workflows/devenv-ci/README.md) uses language-cache discovery and compiled-cache discriminators.
GitHub cache storage is the default on GitHub-hosted runners.
On trusted self-hosted runners, optional S3 cache configuration is split into Actions variables for non-secret endpoints and Actions secrets for credentials.

Configure these under Settings, Secrets and variables, Actions, Variables (automatically inherited by reusable workflows):

- `TARS_CLOUD_S3_ENDPOINT`: S3-compatible endpoint URL.
- `TARS_CLOUD_S3_BUCKET`: target bucket name.
- `TARS_CLOUD_S3_REGION`: bucket region.

Configure these under Settings, Secrets and variables, Actions, Secrets and pass them under `secrets:` on reusable workflow calls:

- `TARS_CLOUD_S3_ACCESS_KEY`: S3 access key ID.
- `TARS_CLOUD_S3_SECRET_ACCESS_KEY`: S3 secret access key.
- `TARS_CLOUD_S3_SESSION_TOKEN`: optional temporary session token.

Partial S3 configuration fails clearly.
Supplied S3 credentials are withheld from untrusted runs.
Post-job saves retain the success-only policy.
