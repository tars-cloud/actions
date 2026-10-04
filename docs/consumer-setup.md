# Consumer Setup and Runners

These interfaces are available in the next v3 release.
Pin its full release commit SHA when adopting them; an existing v3.1.0 SHA does not gain new inputs.

## Runner Selection

Every reusable consumer workflow accepts `runs-on` as a JSON runner label, label array or group/labels object.
Set it once to select the runner for all jobs, including comments, summaries and publishers.
Hosted runners are used when the selector is omitted.
CodeQL retains its hosted macOS default for Swift; Linux environment modes keep their existing platform requirements.
Composite actions inherit their caller job's runner.

```yaml
---
with:
  runs-on: '{"group":"enterprise/tars-cloud","labels":["self-hosted","linux","x64"]}'
```

Cargo CRAP and CodeQL also accept `reporting-runs-on` to override their reporting jobs.
Empty inherits `runs-on`; explicit `"ubuntu-24.04"` selects hosted reporting alongside self-hosted analysis.
CodeQL matrix `runner` values override individual analysis jobs only.
When using matrix-only runner selection, set `runs-on` or `reporting-runs-on` for the summary job too.
An unavailable self-hosted runner queues the job; there is no automatic switch to hosted infrastructure.
The selected runner group must permit the caller repository and workflow.

Repository-only workflows expose JSON selectors through `TARS_RUNNER_AMD64`, `TARS_RUNNER_ARM64` and `TARS_REPORTING_RUNNER` repository variables.
Empty values preserve their hosted architecture defaults.
`TARS_SELF_HOSTED_RUNNER` overrides the existing enterprise runner group used by dedicated integration and automation jobs.
`TARS_S3_RUNNER` overrides the S3 warm-restoration selector; it must match the cold job's architecture.
Dedicated lab integration jobs retain their existing enterprise-group defaults.

## Checkout and Profile

All consumer workflows accept `lfs` and `secretspec-profile`.
LFS defaults to false; an empty profile retains consumer configuration.
Existing workflow submodule and exact-revision checkout behavior is preserved.
The [setup-consumer composite](../composite/setup-consumer/README.md) exposes checkout depth, reference and submodule options directly.

SecretSpec uses the noninteractive `env` provider.
Keep `SECRETSPEC_PROFILE` and the documented action-specific variables in `devenv.yaml` when enabling a clean shell.
Consumers own their SecretSpec manifests and application secret provisioning.

## Private Dependencies

Existing `dependency-token` secrets remain supported.
Alternatively name dependency repositories explicitly and pass dedicated dependency App secrets:

```yaml
---
with:
  dependency-owner: my-organization
  dependency-repositories: '["private-nix-input", "shared-crate"]'
secrets:
  dependency-app-id: ${{ secrets.DEPENDENCY_APP_ID }}
  dependency-app-private-key: ${{ secrets.DEPENDENCY_APP_PRIVATE_KEY }}
```

The App must be installed on the named repositories and permit Contents read access.
Only explicitly named repositories enter the token scope.
Checkout uses the repository token unless the dependency scope explicitly includes the consumer repository under the same owner.
Include it when private LFS or submodules need the App credential.
An empty dependency list with App credentials fails before checkout rather than granting organization-wide access.
The minted token is read-only even when the App installation permits writes.
It is separate from `app-id` and `app-private-key` publication credentials.
Tokens are revoked after the job and are not saved in Git configuration or cache archives.
Fork PRs, Dependabot runs and `pull_request_target` do not receive supplied dependency credentials.

The dependency token is passed to Nix environment setup and subsequent shell entries.
For package registries and application secrets, retain the consumer's own configuration.

## Shared CI Caches

The [consumer devenv CI workflow](../workflows/consumer-devenv-ci/README.md) uses existing language-cache discovery and compiled-cache discriminators.
GitHub cache storage is the default.
Optional S3 credentials use the existing `S3_ENDPOINT`, `S3_BUCKET`, `S3_REGION`, `S3_ACCESS_KEY`, `S3_SECRET_ACCESS_KEY` and `S3_SESSION_TOKEN` secret names.
Partial S3 configuration fails clearly.
Supplied S3 credentials are withheld from untrusted runs.
Post-job saves retain the existing success-only policy.
