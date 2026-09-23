# report-status

Report named step or job results at the end of a pipeline. The action writes a GitHub job summary and optionally
publishes one aggregate commit status. It uses Bash builtins for reporting and `gh` from PATH for publication. It never
enters devenv, installs tools, restores caches or executes consumer project code.

## Step results

Use `if: always()` on the calling step so earlier failures do not skip the report. Pin the action to a reviewed commit
SHA.

```yaml
---
steps:
  - id: report
    name: Report build and validation results
    if: always()
    uses: tars-cloud/actions/report-status@<reviewed-sha>
    with:
      title: Build and validation
      results: |
        build=${{ steps.build.outcome }}
        tests=${{ steps.tests.outcome }}
        upload=${{ steps.upload.outcome }}
      allow-skipped: upload
```

Supply one `name=result` per line. Names start with a letter, digit or underscore and contain only letters, digits,
underscores, dots or hyphens. Names must be unique; their input order is preserved in the summary. Blank lines,
surrounding whitespace and CRLF line endings are accepted. Results must be `success`, `failure`, `cancelled` or
`skipped`. An empty result from a step that never ran is invalid, not a successful check. Report a real outcome rather
than replacing absent results with success.

Use `outcome` to report a failure even when the step has `continue-on-error: true`. Use `conclusion` when the report
should respect that tolerance. The report covers only the results provided by the caller, not later steps or post-job
cleanup.

## Job results and an optional gate

A final job must list every job it reports in `needs` and use `if: always()`. The following job needs no project
environment setup. Using the published remote action does not require a checkout.

```yaml
---
jobs:
  # -------------------------------------------------
  # Pipeline report
  # -------------------------------------------------
  report:
    needs: [build, tests, deploy]
    if: always()
    runs-on:
      group: enterprise/tars-cloud
    timeout-minutes: 5
    permissions: {}
    steps:
      - id: report
        name: Report all required jobs
        uses: tars-cloud/actions/report-status@<reviewed-sha>
        with:
          results: |
            build=${{ needs.build.result }}
            tests=${{ needs.tests.result }}
            deploy=${{ needs.deploy.result }}
          allow-skipped: deploy
          fail-on-error: "true"
```

Reporting is advisory by default and does not change the job exit status for failed pipeline results. Set
`fail-on-error: "true"` to fail after writing the report when its gate does not pass. Every supplied check is required
unless its name appears in `allow-skipped`, one name per line. This allowance applies only to skipping; it never
tolerates a failure or cancellation. Unknown names in `allow-skipped` are configuration errors.

Aggregation has this precedence:

- Invalid input produces `unknown`, prevents publication and fails the action after attempting the summary.
- Any failure produces `failure`.
- Otherwise, any cancellation produces `cancelled`.
- Otherwise, any required skipped check or an entirely skipped report produces `skipped`.
- At least one success with only explicitly allowed skips produces `success` and passes the gate.

An entirely skipped report never passes the gate, even if every check permits skipping. Cancellation reporting is best
effort: a cancelled runner or workflow may never execute the action. Matrix job results provide an aggregate, not
individual matrix entries; pass named results explicitly when that detail is needed.

## Optional commit status

Set `publish-status: "true"` and supply `github-token` with Commit statuses write permission. For `GITHUB_TOKEN`, set
`permissions: statuses: write` on the job using normal YAML nesting. The summary-only action needs no token or GitHub
API access. GitHub App tokens are also supported; their installation must grant Commit statuses write permission.

```yaml
---
permissions:
  statuses: write
steps:
  - id: report
    name: Publish the pipeline result
    if: always()
    uses: tars-cloud/actions/report-status@<reviewed-sha>
    with:
      results: |
        build=${{ steps.build.outcome }}
        tests=${{ steps.tests.outcome }}
      publish-status: "true"
      github-token: ${{ github.token }}
      status-context: CI / aggregate
```

`status-context` defaults to the workflow name; choose distinct contexts for independent reports and matrix entries.
`repository` defaults to the current repository. `sha` defaults to the PR head SHA, otherwise the workflow SHA; an
override must be a full 40-character commit SHA. `run-url` defaults to the workflow run URL and is used in the summary
and commit status. Commit statuses map `success` to success, `failure` to failure, and `cancelled` or
required/all-skipped results to error. Allowed skips alongside a success produce a success status.

The action writes the summary before checking `type gh >/dev/null 2>&1` or making an API request. A missing CLI, missing
token, denied request or network failure produces a warning by default. This includes read-only tokens on fork PRs; do
not expose privileged App credentials to fork code to bypass that restriction. Set `reporting-errors: fail` when
publication or summary-writing errors must fail the reporting step. This policy is separate from `fail-on-error`, which
concerns the pipeline results. API errors never rewrite a successful pipeline result as a failed pipeline result. The
action suppresses API responses and does not print credentials or supplied result text in error annotations.

## Outputs

- `result`: `success`, `failure`, `cancelled`, `skipped` or `unknown`.
- `passed`: `true` only when the configured gate passes.
- `success-count`, `failure-count`, `cancelled-count`, `skipped-count`: counts of valid, uniquely named results.
- `reporting-result`: `success` or `failure` for report generation and requested publication.
- `publication-result`: `success`, `failure` or `skipped` when publication is disabled or inputs are invalid.

If GitHub's output file itself is unavailable, outputs cannot be delivered; the action warns or fails according to
`reporting-errors`. The local summary cannot be written if the runner does not supply a writable `GITHUB_STEP_SUMMARY`
file.

## Tests

`test.yaml` runs the actual Bash implementation through Rust-based Tact with an isolated PATH and mocked `gh` calls.
Cases cover aggregation, optional skips, gating, invalid inputs, summary persistence, missing tools and API failures. No
live credentials are required for these tests. CI also invokes the composite directly on hosted AMD64/ARM64 and
enterprise runners before any project environment setup.
