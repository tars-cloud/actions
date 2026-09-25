# setup-devenv

[Copyable workflow example](example.yaml).

Prepare a direct devenv environment or a devenv-integrated devShell without dependency archive or Cachix setup.
Supports GitHub.com Linux X64 and ARM64 runners 2.336.0+.
It invokes setup-nix at the same action revision.

```yaml
- uses: tars-cloud/actions/composite/setup-devenv@v1
  with:
    working-directory: .
    warmup: "true"
```

- `type`: `devenv` by default, or explicitly `flakes`.
- `flake-shell`: `.#default`; used only for flakes.
- `working-directory`: `.` relative to the checkout; the consumer environment root.
- `system`: empty defaults to the runner system; accepts `x86_64-linux` or `aarch64-linux`.
- `devenv-installable`: optional Nix installable for the native CLI; only supported in direct mode.
- `warmup`: `true`; `false` skips native warmup, but foreign-system validation still enters the shell.
- `github-token`: defaults to `github.token`; use a separately generated token for private dependency access where
  needed.
- `devenv-version` output: selected CLI version, including an explicit pin; empty in flake mode.
- `system` output: resolved execution system, including when the input defaults to the native runner.

Direct mode requires devenv.nix, devenv.yaml and devenv.lock.
Without `devenv-installable`, it reuses devenv, installing through `nix profile add nixpkgs#devenv` only when missing.
An explicit installable always takes precedence, even when another CLI is installed.
Use a full revision such as `github:cachix/devenv/<full-commit-sha>` for reproducibility.
The action builds it into a unique directory under `runner.temp`, adds its executable directory to the job PATH, and leaves shared profiles unchanged.
Installation failures fail the action; an existing CLI is not used as a fallback.
The CLI runs natively even when the project environment uses emulation.
Consumer lockfiles continue to select the environment's dependencies.
Flake mode requires flake.nix and flake.lock and rejects `devenv-installable`, since it does not use the standalone CLI.
Both modes warm with a trivial command; consumer shell hooks still run and preparation failures propagate.

An explicit `system` becomes `devenv --system` or `nix develop --system` for shell entry.
Foreign-system execution requires preconfigured runner emulation and a matching Nix `extra-platforms` entry.
The action checks the configuration and executes a shell probe, including when `warmup` is false.
It does not install binfmt handlers, use sudo, or configure remote builders.
Compiler and release-binary architecture checks remain the consumer's responsibility.

Commands use argument arrays with strict Bash handling:

```bash
devenv --no-tui shell --quiet -- bash --noprofile --norc -euo pipefail -c ':'
nix develop --impure .#default --command bash --noprofile --norc -euo pipefail -c ':'
```

Warmup uses `SECRETSPEC_PROVIDER=env` to prevent interactive credential prompts.
Explicit profile, reason and credential variables are preserved; CI and reason receive noninteractive defaults when absent.
No secretspec installation or deployment-secret lookup is performed.
The dependency token is scoped to shell entry through process environment Nix configuration; it is not written to repository files or cache archives.
The hosted Nix installer can configure its bootstrap token using its own standard behaviour.
The action does not mint GitHub App tokens.

Use [run-devenv](../run-devenv/README.md) for subsequent commands, passing the same environment and system inputs.
Pass `system` to setup-cache too when using emulation so compiled caches remain separate.
An ordinary workflow `run` step does not automatically enter the prepared environment.
Run setup-cache first when caches are wanted; setup-devenv does not require it.
Run [setup-nix-cache](../setup-nix-cache/README.md) before this action when optional Cachix access is wanted.
