# Shared CodeQL workflow

[Copyable workflow example](example.yaml) · [Workflow](../../.github/workflows/codeql.yml)

Analyze a consumer repository with one CodeQL job per language and an aggregate status job.
Select `type: devenv` or `type: flakes` to use the consumer's declared toolchains on Linux.
The workflow composes setup-devenv, run-devenv and report-status at the selected workflow revision.
The default `type: runner` preserves existing callers and supports macOS/Swift.

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

Rust supports only `none` and requires `cargo` and `rustup` in the selected environment.
No-build analysis still processes build scripts and procedural macros; it does not mean no toolchain.
Swift requires macOS and supports `autobuild` or `manual`.
Go supports `autobuild` or `manual`.
C/C++, C# and Java support `none`, `autobuild` or `manual`; Kotlin requires a build.
The workflow rejects unsupported language/mode combinations before CodeQL initialization.
See [CodeQL build requirements](https://docs.github.com/en/code-security/reference/code-scanning/codeql/build-options-for-compiled-languages).

## Inputs

- `languages`: JSON language array.
- `analysis-matrix`: optional JSON object containing `include` rows; replaces `languages` entirely.
- `runs-on`: optional JSON runner label, label array or group/labels object; defaults to `ubuntu-24.04`, except Swift defaults to `macos-15`.
- `type`: `runner` by default for compatibility; use `devenv` or `flakes` for a declared consumer environment on Linux.
- `working-directory`: consumer environment root and setup/manual-build directory, default `.`; CodeQL still analyzes the checkout.
- `flake-shell`: selected devShell, default `.#default`, when `type` is `flakes`.
- `devenv-installable`: optional pinned Nix installable for the direct-mode CLI; overrides an installed CLI.
- `export-variables`: additional whitespace-separated non-secret environment variable names to expose to CodeQL's upstream actions.
- `config-file`: `auto` discovers `.github/codeql-config.yml` in the consumer checkout; empty uses CodeQL defaults; explicit relative paths must exist.
- `setup-command`: optional Bash command before initialization for consumer build dependencies.
- `build-command`: optional Bash command between initialization and analysis, only with `manual` build mode.
- `submodules`: default `false`; enable recursive checkout when required.
- `upload-sarif`: default `true`; `false` still analyzes but does not upload.
- `publish-status`: default `false`; additionally publish an aggregate commit status using `gh` when the caller grants `statuses: write`.
- `status-context`: optional commit status name, default `CodeQL Status`.

Each advanced matrix row requires `language` and may override `build-mode`, `runner`, `type`, `setup-command` and `build-command`.
`runner` is a JSON string, array or group/labels object, not a JSON-encoded string inside the row.
An example input block for mixed builds is:

```yaml
---
with:
  type: devenv
  analysis-matrix: >-
    {"include":[
      {"language":"actions"},
      {"language":"c-cpp","build-mode":"manual","build-command":"make -j2"},
      {"language":"swift","type":"runner","build-mode":"manual","runner":"macos-15","build-command":"swift build"}
    ]}
```

Setup and build commands are intentional consumer-authored Bash programs, passed through environment variables.
They execute from `working-directory` with strict error handling.
In devenv/flake mode they run through run-devenv; in runner mode they run in the runner's Bash environment.
A setup command can export settings to later steps through `GITHUB_ENV` and `GITHUB_PATH`.
In runner mode, toolchain provisioning belongs to the runner or the explicit setup command.
In devenv/flake mode, declare packages in the consumer environment; the shared workflow does not download missing project tools.
The matrix job supports Linux and macOS; the aggregate report uses `runs-on` or Ubuntu when unspecified.

## Devenv and flake toolchains

Direct mode requires `devenv.nix`, `devenv.yaml` and `devenv.lock` in `working-directory`.
Flake mode requires `flake.nix` and `flake.lock` and uses the selected devShell.
Both run on the runner's native Linux architecture; emulated analysis is not supported.
The shell is prepared separately for each language job, including its shell hooks.
Use a CI-compatible environment that can start without interactive credentials.

For Rust, enable the Rust language module and include `pkgs.rustup` in `packages`.
Keep Cargo, rustc and source components selected by the consumer's locked environment.
The workflow fails before initialization if Cargo or rustup is only available from the inherited runner PATH.
C/C++ automatic system-dependency installation is disabled in these modes; declare compiler and build dependencies in the consumer environment.

CodeQL's init, autobuild and analyze steps are JavaScript actions and cannot use a workflow `shell` override.
The workflow enters the consumer shell and forwards its ordered PATH and selected compiler/runtime variables through GitHub's environment files.
These include Rust source/toolchain paths, Nix compiler flags, C/C++ tools, Java/.NET roots and language runtime paths.
The same environment is then visible to the subprocesses that CodeQL starts during extraction.
Only selected variables are exported; arbitrary shell variables, Nix access-token configuration and credentials are not copied.
Use `export-variables` for additional non-secret build settings your project needs, such as `MY_LIBRARY_ROOT`.
GitHub, runner and action control variables cannot be selected for export.
Setup commands can also deliberately publish settings through `GITHUB_ENV` and `GITHUB_PATH`.

The workflow does not configure Cachix or a separate Nix cache.
Keep Nix substitution and Cachix policy in the consumer's environment or runner configuration.

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
