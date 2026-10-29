# Database Mirrors

[tact.yaml](../tact.yaml) declares OCI database packages and their ordered sources.
[Repository: Refresh Database Mirrors](../.github/workflows/repository-mirrors.yaml) runs at 00:00, 06:00, 12:00 and 18:00 UTC, or manually from `trunk`.
GitHub can delay or drop scheduled jobs during high load, so the existing published version remains available when a refresh is missed.

## Configuration

The root configuration has a format `version` and separate sections for Tact commands.
Version 1 implements `mirrors`; future sections can configure other commands.
[schemas/tact.schema.json](../schemas/tact.schema.json) supplies editor validation.
Unknown fields, duplicate names and destinations, empty lists, credential-bearing references and reserved `tact-` tags are rejected.

- `mirrors.retention-days`: grace period for superseded versions, from 1 to 365 days.
- `mirrors.timeout-seconds`: deadline for each external command, default 300, maximum 1800.
- `mirrors.packages`: packages to refresh independently, in declaration order.
- `name`: unique package label used in logs.
- `kind`: `oci` in version 1.
- `destination`: GHCR organisation package without a tag or digest.
  It must begin with `ghcr.io/<GITHUB_REPOSITORY>/`.
- `tags`: public tags to move after verifying the uploaded artifact.
- `sources`: registry references with explicit tags or SHA-256 digests, attempted in order until a complete download succeeds.

Add another package to this list to mirror another OCI database.
HTTP downloads, conversion of antivirus definitions, image builds and repackaging are outside version 1.
The mirror copies the upstream manifest and blobs without changing their media types or annotations.

Validate without contacting a registry or changing packages:

```bash
CI=true SECRETSPEC_PROVIDER=env SECRETSPEC_REASON=validate-database-mirrors \
  devenv --no-tui shell --quiet -- tact mirror --check
```

`tact validate` also validates an existing root `tact.yaml`.
`tact mirror --config path/to/config.yaml` selects another configuration, relative to `--root` unless absolute.
Live refreshes require the repository devenv shell, ORAS and `gh` on its PATH, `GITHUB_REPOSITORY`, and authenticated write access to the configured packages.
The workflow supplies a job-scoped `GITHUB_TOKEN` with `packages: write`.
It creates a private temporary ORAS authentication file outside cache paths and removes it on exit.
`TACT_ORAS_CONFIG` supplies this file to each ORAS registry operation.
The command does not install missing tools or use a Docker daemon.

## Publication and Retention

Each source is resolved to a digest before downloading into a separate temporary OCI layout.
ORAS checks content digests during the copy, and Tact verifies the local manifest digest against the resolved source.
A failed resolution, interrupted download, timeout or digest mismatch advances to the next configured source.
The first complete verified download is selected; Tact does not compare freshness across upstream mirrors.

The selected layout is uploaded to `tact-staging` and its destination digest is checked before public tags move.
Superseded public digests receive `tact-retained-<unix-seconds>-<version-id>` tags before promotion.
This measures retention from replacement rather than the artifact's original creation date.
Unchanged digests do not create new package versions or retention tags.
Each public tag is resolved again, and GitHub's paginated package metadata must confirm the new tags before deletion starts.

Pruning removes superseded versions whose latest retention timestamp is older than three days.
Old untagged staging leftovers use their package creation time.
Current versions, versions retired in this run, and versions with other tags are protected.
The current version remains protected even if it is older than the retention period.
The grace period protects in-flight pulls and provides rollback history; it does not make an old database fresh.

If all sources fail, public tags and retained versions remain untouched.
Upload, promotion, verification or cleanup failures fail that package's refresh.
A multi-tag promotion can partially complete if the registry fails between tag updates; both digests remain retained and pruning is skipped.
Other packages are still attempted, and any package failure fails the overall job.
The workflow serializes refreshes and cannot publish from pull requests or forks.
Avoid running another writer against these same packages during a refresh.

GitHub requires package administration rights for version deletion.
A repository publishing with its `GITHUB_TOKEN` receives those rights by default; existing packages may need the repository granted access.
GitHub can refuse deletion of public versions with more than 5,000 downloads.
Such cleanup failures remain visible as failed refresh jobs and do not remove the current version.
See [package permissions](https://docs.github.com/en/packages/learn-github-packages/about-permissions-for-github-packages) and [deletion restrictions](https://docs.github.com/en/packages/learn-github-packages/deleting-and-restoring-a-package).

## Enable Anonymous Downloads

After merging, manually run **Repository: Refresh Database Mirrors** on `trunk` or wait for its schedule.
Pull requests do not publish packages.
The first publish creates packages with private visibility, even though the source repository is public.

For each package, open its settings, connect it to `tars-cloud/actions` if necessary, and change its visibility to **Public**:

- `actions/trivy-db`
- `actions/trivy-java-db`
- `actions/trivy-checks`

Confirm anonymous pulls before configuring consumers.
GitHub's package API does not provide a visibility-update operation; this is a package settings step.
See [GHCR visibility and repository linking](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry).

## Consumer Trivy Configuration

After the packages are public, add these settings to the consumer's `trivy.yaml`:

```yaml
---
db:
  repository:
    - ghcr.io/tars-cloud/actions/trivy-db:2
    - ghcr.io/aquasecurity/trivy-db:2
    - docker.io/aquasec/trivy-db:2
    - public.ecr.aws/aquasecurity/trivy-db:2
java-db:
  repository:
    - ghcr.io/tars-cloud/actions/trivy-java-db:1
    - ghcr.io/aquasecurity/trivy-java-db:1
    - docker.io/aquasec/trivy-java-db:1
    - public.ecr.aws/aquasecurity/trivy-java-db:1
misconfiguration:
  checks-bundle-repository: ghcr.io/tars-cloud/actions/trivy-checks:2
```

The `daily` alias is also published for each package, and is refreshed every six hours.
The numbered tags preserve Trivy's schema and checks-major compatibility conventions.
Trivy 0.74 uses checks bundle major 2.
Review these tags when upgrading Trivy across a database schema or checks-major change.

Trivy's vulnerability and Java database fallback applies to transient registry errors, not every failure.
A missing or private first repository can stop a scan without trying the next source.
Checks accept one repository and fall back to embedded checks when their download fails.
Repository self-scans therefore use official sources during bootstrap, independently of package publication and visibility.
Consumers can use this same list in both their reporting and gating configuration files.
Neither workflow disables Trivy's normal database freshness checks.

## GitHub Cache Reuse

`consumer-trivy` already uses `setup-cache` to restore and save the Trivy cache directory.
GitHub-hosted runners use GitHub storage; trusted self-hosted runners use the caller's S3 configuration.
Keys rotate daily in UTC and restore compatible older entries.
Only successful jobs save caches, and an existing GitHub cache entry cannot be overwritten.
Restoring a database does not disable Trivy's freshness checks.

GitHub caches are scoped to the consuming repository and branch access rules.
They reduce repeated downloads within each repository; they cannot supply a shared cache across hundreds of repositories.
The public OCI packages provide that common source, while the existing cache avoids downloading it on every scan.
See [Trivy cache contents](https://trivy.dev/docs/dev/guide/configuration/cache/) and [GitHub cache scope](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching).

## Upstream Sources

Vulnerability and Java databases use GHCR, Docker Hub and public ECR, in that order.
Checks use GHCR and Docker Hub.
The checks release workflow does not publish to ECR, and `public.ecr.aws/aquasecurity/trivy-checks:2` returned 404 during research.
Google's pull-through mirror is documented by Trivy but is excluded from this configuration.

- [Trivy database locations and fallback](https://trivy.dev/docs/latest/configuration/db/).
- [Trivy self-hosting](https://trivy.dev/docs/latest/advanced/self-hosting/).
- [Vulnerability database build schedule](https://github.com/aquasecurity/trivy-db/blob/main/.github/workflows/cron.yml).
- [Checks publishing workflow](https://github.com/aquasecurity/trivy-checks/blob/main/.github/workflows/reusable-release.yaml).
- [Trivy 0.74 checks version](https://github.com/aquasecurity/trivy/blob/v0.74.0/pkg/policy/policy.go).
- [ORAS copy operations](https://oras.land/docs/commands/oras_cp/).

## Repository Security Scans

[Repository: SecOps](../.github/workflows/repository-secops.yaml) scans pull requests, merge groups, trunk pushes and a daily schedule.
It uses this revision's reusable Trivy and CodeQL workflows with the repository's direct devenv environment.
Trivy reports vulnerabilities, secrets, misconfigurations and licences at all severities.
The separate gate blocks HIGH and CRITICAL vulnerabilities, secrets and misconfigurations.
Licence findings are reporting only, and setup or scanner failures always fail the job.
Generated Nix state, build output, Git internals and scratch files are excluded.
CodeQL analyzes Actions, Rust and JavaScript/TypeScript.
