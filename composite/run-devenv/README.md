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
- No outputs.

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
