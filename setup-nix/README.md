# setup-nix

Ensure Nix is available before any project-shell or optional Cachix setup. Supports GitHub.com Linux X64 and ARM64
runners 2.336.0+.

```yaml
- uses: tars-cloud/actions/setup-nix@v1
```

- `github-token`: defaults to `github.token`; used only for missing hosted Nix bootstrap.
- Output `installed`: `true` when this call requested hosted installation, otherwise `false`.

Existing Nix is checked with `nix --version` and otherwise left untouched. A broken existing executable fails visibly.
Missing hosted Nix uses cachix/install-nix-action v31.11.1 at a full commit pin. Missing self-hosted Nix fails with
instructions to install it on the runner. Repeated calls do not install, upgrade, modify the registry or reset existing
Nix configuration. This action does not install devenv, Cachix, secretspec or project tools. See
[shared runtime requirements](../README.md#runtime-requirements).
