# Shared CodeQL workflow

[Copyable workflow example](example.yaml) · [Workflow](../../.github/workflows/codeql.yml)

Analyze a consumer repository with one CodeQL job per language and an aggregate status job.
CodeQL uses the runner's toolchains and does not install Nix or enter devenv.
The reporting job reuses this repository's report-status composite at the selected workflow revision.

The example targets the upcoming `v2` alias.
Replace it with the full revision under test until publication, then pin a published release SHA with a matching version comment.
The consumer owns events, branch filters, concurrency and token permissions.
Grant `contents: read`, `actions: read` and `security-events: write` for uploaded analysis.
Private packages may additionally require `packages: read`.
Code scanning must be available and advanced CodeQL setup enabled for the consumer; do not run competing default and advanced setups.

## Language selection

`languages` is a JSON array of canonical CodeQL identifiers, default `["actions"]`.
Supported identifiers are `actions`, `c-cpp`, `csharp`, `go`, `java-kotlin`, `javascript-typescript`, `python`, `ruby`, `rust` and `swift`.
The matrix runs every selected language even if another fails.

The default build mode is `none` for Actions, JavaScript/TypeScript, Python, Ruby, Rust, C/C++ and C#.
Go, Java/Kotlin and Swift default to `autobuild`.
Java/Kotlin defaults to a build so Kotlin is not silently omitted.
No-build C/C++ analysis can be less precise for projects requiring generated code or custom compiler settings.

Rust supports only `none` and requires `cargo` and `rustup` on the runner.
Swift requires macOS and supports `autobuild` or `manual`.
Go supports `autobuild` or `manual`.
C/C++, C# and Java support `none`, `autobuild` or `manual`; Kotlin requires a build.
The workflow rejects unsupported language/mode combinations before CodeQL initialization.
See [CodeQL build requirements](https://docs.github.com/en/code-security/reference/code-scanning/codeql/build-options-for-compiled-languages).

## Inputs

- `languages`: JSON language array.
- `analysis-matrix`: optional JSON object containing `include` rows; replaces `languages` entirely.
- `runs-on`: optional JSON runner label, label array or group/labels object; defaults to `ubuntu-24.04`, except Swift defaults to `macos-15`.
- `config-file`: `auto` discovers `.github/codeql-config.yml` in the consumer checkout; empty uses CodeQL defaults; explicit relative paths must exist.
- `setup-command`: optional Bash command before initialization for consumer build dependencies.
- `build-command`: optional Bash command between initialization and analysis, only with `manual` build mode.
- `submodules`: default `false`; enable recursive checkout when required.
- `upload-sarif`: default `true`; `false` still analyzes but does not upload.
- `publish-status`: default `false`; additionally publish an aggregate commit status using `gh` when the caller grants `statuses: write`.
- `status-context`: optional commit status name, default `CodeQL Status`.

Each advanced matrix row requires `language` and may override `build-mode`, `runner`, `setup-command` and `build-command`.
`runner` is a JSON string, array or group/labels object, not a JSON-encoded string inside the row.
An example input block for mixed builds is:

```yaml
---
with:
  analysis-matrix: >-
    {"include":[
      {"language":"actions"},
      {"language":"c-cpp","build-mode":"manual","build-command":"make -j2"},
      {"language":"swift","build-mode":"manual","runner":"macos-15","build-command":"swift build"}
    ]}
```

Setup and build commands are intentional consumer-authored Bash programs, passed through environment variables.
They execute from the checkout root with strict error handling.
A setup command can export settings to later steps through `GITHUB_ENV` and `GITHUB_PATH`.
Toolchain provisioning belongs to the runner or the explicit setup command; the shared workflow does not guess project dependency installation commands.
The matrix job supports Linux and macOS; the aggregate report uses `runs-on` or Ubuntu when unspecified.

## Configuration ownership and results

Checkout fetches the consumer repository, so `.github/codeql-config.yml` comes from that repository.
Query selection and path exclusions belong in that configuration.
An explicitly missing file fails rather than silently dropping the requested policy.
The first interface accepts local configuration files; shared policy files can be committed by consumers without coupling configuration updates to workflow implementation.

Each analysis keeps the category `/language:<identifier>`.
Specify a language only once in a call to avoid competing uploads for the same category.
The `result` workflow output aggregates all language jobs, including setup, build and upload failures.
The aggregate job is named `CodeQL - Summary`.
CodeQL uploads findings but does not fail merely because an alert exists; use GitHub code-scanning merge protection for alert-based gating.
When migrating existing branch protection, check the complete check names generated by the caller and update required checks accordingly.
