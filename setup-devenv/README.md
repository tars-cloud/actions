# setup-devenv

Prepare a direct devenv environment or a devenv-integrated devShell without dependency archive or Cachix setup. Supports
GitHub.com Linux X64 and ARM64 runners 2.336.0+. It invokes setup-nix at the same action revision.

```yaml
- uses: tars-cloud/actions/setup-devenv@v1
  with:
    working-directory: .
    warmup: "true"
```

- `type`: `devenv` by default, or explicitly `flakes`.
- `flake-shell`: `.#default`; used only for flakes.
- `working-directory`: `.` relative to the checkout; the consumer environment root.
- `warmup`: `true`; `false` skips entering the shell.
- `github-token`: defaults to `github.token`; use a separately generated token for private dependency access where
  needed.
- No outputs.

Direct mode requires devenv.nix, devenv.yaml and devenv.lock. It reuses devenv, installing through
`nix profile add nixpkgs#devenv` only when missing. There is no action-managed version pin or channel override. Flake
mode requires flake.nix and flake.lock and never installs the standalone devenv CLI. Both modes warm with a trivial
command; consumer shell hooks still run and preparation failures propagate.

Commands use argument arrays with strict Bash handling:

```bash
devenv --no-tui shell --quiet -- bash --noprofile --norc -euo pipefail -c ':'
nix develop --impure .#default --command bash --noprofile --norc -euo pipefail -c ':'
```

Warmup uses `SECRETSPEC_PROVIDER=env` to prevent interactive credential prompts. Explicit profile, reason and credential
variables are preserved; CI and reason receive noninteractive defaults when absent. No secretspec installation or
deployment-secret lookup is performed. The dependency token is scoped to shell entry through process environment Nix
configuration; it is not written to repository files or cache archives. The hosted Nix installer can configure its
bootstrap token using its own standard behaviour. The action does not mint GitHub App tokens.

Subsequent workflow steps still need an explicit [direct or flake shell](../README.md). Run setup-cache first when
caches are wanted; setup-devenv does not require it.
