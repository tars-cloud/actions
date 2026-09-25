# run-devenv

[Copyable workflow example](example.yaml).

Run Bash commands inside a direct devenv environment or a flake devShell.
Prepare Nix and the CLI with [setup-devenv](../setup-devenv/README.md) first.
This action does not install tools or configure caches.

```yaml
- id: test
  name: Test the consumer project
  uses: tars-cloud/actions/composite/run-devenv@<reviewed-sha>
  with:
    system: ${{ matrix.system }}
    working-directory: .
    run: |
      cargo test --locked
      trivy --version
```

- `run`: required Bash source; multiline commands are supported.
- `system`: empty defaults to the runner system; accepts `x86_64-linux` or `aarch64-linux`.
- `type`: `devenv` by default, or `flakes`.
- `flake-shell`: `.#default`; used only for flakes.
- `working-directory`: consumer environment root, relative to the checkout; defaults to `.`.
- `github-token`: defaults to `github.token` for dependency access during shell entry.
- `result` output: a JSON object written by the command, or `{}` when no result file is written.

Pass the same `system`, `type`, `flake-shell` and `working-directory` used for setup.
Setup does not change the execution system of later steps automatically.
Foreign systems require existing runner emulation and Nix `extra-platforms` configuration.
A trivial shell probe runs before foreign-system commands, so a runner configuration failure is reported before the supplied commands execute.
No privileged configuration is performed.

The inner Bash uses `--noprofile --norc -euo pipefail` and propagates its exit code.
The action sets noninteractive SecretSpec defaults and preserves an explicitly selected profile and reason.
Workflow environment variables and cache paths are inherited by the consumer shell.
Use workflow `env` for untrusted values and reference them as quoted shell variables inside `run`.
The `run` input is executable source, just like a workflow's normal `run` step.

## Structured results

Available after v1.0.0, in the next minor release.
The action supplies `DEVENV_RESULT_FILE`, an absolute path in a unique directory under the runner's temporary directory.
Write one UTF-8 JSON object to that file to return build metadata to the calling workflow.
The file starts absent, and not writing it returns `{}`.
An empty file, malformed JSON, a non-object value, a symlink, or an oversized result fails the action.
The file and the published output must each fit within 64 KiB; the output limit counts UTF-16 bytes, matching GitHub's output accounting.
Use artifacts for larger data.

```yaml
- id: build
  name: Build and return artifact metadata
  uses: tars-cloud/actions/composite/run-devenv@<next-release-sha>
  env:
    BINARY_NAME: ${{ matrix.binary_name }}
  with:
    run: |
      cargo build --release
      jq -n --arg binary "$BINARY_NAME" \
        '{binary_name: $binary}' > "$DEVENV_RESULT_FILE"

- id: inspect
  name: Use the returned metadata
  shell: bash
  env:
    BINARY_NAME: ${{ fromJSON(steps.build.outputs.result).binary_name }}
  run: printf 'Built %s\n' "$BINARY_NAME"
```

Declare tools used to construct results, such as `jq` in this example, in the consumer's environment.
Validation uses GitHub's Node action runtime and requires no additional consumer tool.
Normal stdout and stderr continue streaming; result contents are not logged by the action.
GitHub outputs are workflow metadata, so do not put credentials in the result.
Use the documented file instead of writing arbitrary output names to the internal step's `GITHUB_OUTPUT`.

Results are published only after successful command execution and validation.
Failed, cancelled or skipped commands publish no result, and command failures retain their exit code.
The collection step removes the invocation's temporary directory even when commands or JSON validation fail; forced runner termination can prevent cleanup.
It refuses to follow a result directory replaced with a symlink.
Successive invocations use separate files and cannot reuse an earlier result.
The result file is available to the command only; later steps consume `steps.<id>.outputs.result`.
