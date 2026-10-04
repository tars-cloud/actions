# Shared Trivy workflow

[Copyable SecOps example](example.yaml) · [Workflow](../../.github/workflows/consumer-trivy.yaml)

Run Trivy from the consumer's declared devenv environment, restore its database cache, and upload SARIF to the consumer repository.
The workflow calls this repository's setup-cache, setup-devenv, setup-trivy, run-devenv and report-status composites at the caller-selected revision.
It never downloads an independent Trivy binary.
Consumers with `clean.enabled` must retain the [Trivy environment variables](../../docs/clean-environments.md).

The example targets the `v3` alias.
Before publication, replace that alias with the full revision under test.
For production, pin a published release's full commit SHA and add its version as a same-line comment for Dependabot.
Change the example's branch filters and language selection for your repository.

## Consumer contract

Declare `pkgs.trivy` in the selected devenv module and commit its environment lockfile.
Ordinary commands and Trivy run inside that environment.
The workflow uses direct devenv by default and also supports flake-integrated devShells.
The consumer owns triggers, concurrency and token permissions.
Use `pull_request` for PR scans, including Dependabot PRs, and `push` for the default branch.

Grant `contents: read`, `actions: read` and `security-events: write` when uploading SARIF.
GitHub code scanning must be available for the consumer repository.
Set `upload-sarif: false` to scan without uploading.
The reusable workflow inherits the caller's permissions and does not elevate them.

## Inputs

- `job-name`: scan job display name, default `Trivy Scan`; set a distinct name for each invocation in a matrix or multi-scan workflow.
- `runs-on`: JSON runner label, label array or group/labels object, default `"ubuntu-24.04"`.
- `timeout-minutes`: total job limit, default `30`; allow enough time for environment setup and cache cleanup as well as scanning.
- `type`: `devenv` by default, or `flakes`.
- `working-directory`: environment root relative to the checkout, default `.`.
- `flake-shell`: flake selector, default `.#default`.
- `system`: optional `x86_64-linux` or `aarch64-linux`, matching the selected runner or its existing emulation.
- `devenv-installable`: optional pinned CLI reference for direct mode.
- `config-file`: `auto` discovers `trivy.yaml` in the environment root; an explicit relative path must exist; empty disables config discovery.
- `gate-config-file`: optional separate findings policy; empty preserves existing single-scan behavior.
- `scan-path`: existing scan target relative to the environment root, default `.`.
- `fail-on-findings`: default `true`; findings matching the selected configuration fail the job.
- `upload-sarif`: default `true`; upload a generated report even when findings fail the scan.
- `sarif-category`: default `trivy-filesystem`; use distinct values for multiple scans of one commit.
- `submodules`: default `false`; `true` checks out submodules recursively.
- `publish-status`: default `false`; enable only when the caller grants `statuses: write` and provides `gh` on the runner.
- `status-context`: optional commit status name, default `Trivy Scan`.

Environment and scan paths must remain within the consumer checkout.
Configuration and scan targets must remain within the selected environment directory.
File paths are passed as arguments, never interpolated into shell source.
Trivy CLI flags fix the output format, report location, cache directory and finding exit code; other scan policy comes from the consumer configuration.
With no configuration file, Trivy's default scan policy applies.
`fail-on-findings: false` does not suppress scanner execution errors.

## Separate Reporting and Gating

Keep comprehensive reporting and blocking policy in separate consumer-owned files:

```yaml
---
with:
  config-file: .github/trivy/report.yaml
  gate-config-file: .github/trivy/gate.yaml
```

The reporting scan generates SARIF with findings gating disabled.
The gate scan uses its own scanner, severity and exclusion policy with the same target, declared Trivy binary and database cache.
For example, reporting can enable vulnerabilities, misconfigurations, secrets and licences while gating only HIGH/CRITICAL secrets and misconfigurations.
See the [report fixture](../../tests/fixtures/flakes/trivy-report.yaml) and [gate fixture](../../tests/fixtures/flakes/trivy-gate.yaml) for copyable policy files.
Both configuration paths must remain inside the environment directory.
SARIF upload and cleanup run before the findings verdict is enforced, including when gate findings fail CI.
Reporting or gate scanner errors fail the job; they cannot be accepted as findings-only results.
The gate uses exit code 10 for completed findings, separately from scanner execution errors.
`fail-on-findings: false` reports gate findings without blocking; scanner errors remain failures.
Leave `gate-config-file` unset to retain the existing policy and one scan.

All workflows support the [common consumer setup inputs](../../docs/consumer-setup.md): self-hosted runner selectors, LFS checkout, SecretSpec profile and scoped dependency App authentication.

## Cache and secrets

Only Trivy caches are selected, using setup-cache's existing keys and success-only post-job saves.
Failed scans do not save cache archives.
Without S3 configuration, caches use GitHub storage; complete S3 configuration uses S3, and partial configuration fails clearly.

Pass optional S3 secrets explicitly through the caller's job `secrets` mapping:

```yaml
---
secrets:
  S3_ENDPOINT: ${{ secrets.S3_ENDPOINT }}
  S3_BUCKET: ${{ secrets.S3_BUCKET }}
  S3_REGION: ${{ secrets.S3_REGION }}
  S3_ACCESS_KEY: ${{ secrets.S3_ACCESS_KEY }}
  S3_SECRET_ACCESS_KEY: ${{ secrets.S3_SECRET_ACCESS_KEY }}
  S3_SESSION_TOKEN: ${{ secrets.S3_SESSION_TOKEN }}
```

`S3_SESSION_TOKEN` is optional even when S3 is enabled.
The optional `dependency-token` secret supplies access to private GitHub Nix inputs; otherwise shell entry uses `github.token`.
Fork PRs and Dependabot actors or PR authors receive neither the S3 secrets nor the supplied dependency token in setup steps.
This applies to reruns and PR-bearing events as well.
Choose runners permitted to execute the consumer's PR code; a reusable workflow does not grant access to another organization's runner group.

For the lz-cli runner arrangement, set this input:

```yaml
---
with:
  runs-on: '{"group":"enterprise/bingamon-lab","labels":["self-hosted","linux","x64"]}'
```

## Results

The `result` workflow output reports aggregate setup, scan, upload and cleanup outcomes.
Normal workflow checks are the default; custom commit status publication is optional.
The report includes infrastructure failures rather than only the scanner step.
Temporary SARIF files are unique per invocation, uploaded only when nonempty, and removed after use.

Trivy's executable version remains controlled by the consumer's Nix inputs.
Updating the shared workflow revision does not update a consumer's lockfile.
