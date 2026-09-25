# setup-trivy

[Copyable workflow example](example.yaml).

Validate Trivy inside the selected project environment and report its version. Supports GitHub.com Linux X64 and ARM64
runners 2.336.0+. Nix and, for direct mode, devenv must already be available. Run setup-devenv first when bootstrap is
needed.

```yaml
- id: trivy
  uses: tars-cloud/actions/composite/setup-trivy@v1
  with:
    type: devenv
    working-directory: .
```

- `type`: `devenv` by default, or `flakes`.
- `system`: optional `x86_64-linux` or `aarch64-linux`; use the same value as setup-devenv.
  Foreign execution requires preconfigured emulation and Nix `extra-platforms`, verified with a shell probe.
- `flake-shell`: `.#default`; used only for flakes.
- `working-directory`: `.`; consumer environment root.
- `github-token`: defaults to `github.token`; pass the same dependency token as setup-devenv and run-devenv when the environment requires private GitHub inputs.
- Output `version`: Trivy's reported version, such as `0.74.0`.

The token is passed through process-scoped Nix configuration during shell entry, without writing it to repository files or cache archives.

Add `pkgs.trivy` to the selected devenv module's packages and maintain its environment lockfile. The action does not
install Trivy, accept a version override, scan files, fetch a database or restore a cache. Use setup-cache separately
for Trivy download reuse. Call your own scan and report-upload steps through the selected shell.

The availability check restricts lookup to paths prepended by the project shell before the inherited runner PATH
boundary. An ambient runner Trivy, including one installed through Nix, cannot satisfy the check. If custom shell hooks
remove or reorder that boundary, validation fails rather than accepting an unverified ambient binary.
