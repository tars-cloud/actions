# setup-nix

[Copyable workflow example](example.yaml).

Ensure Nix is available before project-shell setup. Supports GitHub.com Linux X64 and ARM64
runners 2.336.0+.

```yaml
---
- id: setup_nix
  name: Set Up Nix
  uses: tars-cloud/actions/composite/setup-nix@v3
```

- `github-token`: defaults to `github.token`; used only for missing hosted Nix bootstrap.
- Output `installed`: `true` when this call requested hosted installation, otherwise `false`.

Existing Nix is checked with `nix --version` and otherwise left untouched. A broken existing executable fails visibly.
Missing hosted Nix uses cachix/install-nix-action v31.11.1 at a full commit pin. Missing self-hosted Nix fails with
instructions to install it on the runner. Repeated calls do not install, upgrade, modify the registry or reset existing
Nix configuration. This action does not install devenv, secretspec or project tools. See
[shared runtime requirements](../../README.md#runtime-requirements).

## Debug Logging

Set `debug: true` to enable verbose tool logs and phase diagnostics.
GitHub's **Re-run with debug logging** enables the same behaviour automatically.
The default is `false`; GitHub debug logging still takes precedence.
See [the debug contract](../../README.md#debug-logging) for details.
