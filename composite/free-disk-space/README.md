# free-disk-space

[Copyable workflow example](example.yaml).

Explicit cleanup for disposable GitHub-hosted Linux X64 and ARM64 runners 2.336.0+.
Self-hosted runners always skip with a notice; there is no override.
Unsupported operating systems and architectures fail before cleanup.

```yaml
---
- id: free_disk_space
  name: Free Disk Space
  uses: tars-cloud/actions/composite/free-disk-space@v4
  with:
    android: "true"
    dotnet: "true"
    haskell: "true"
    boost: "true"
    swift: "true"
```

- `android`: defaults to `true`; removes `/usr/local/lib/android`.
- `dotnet`: defaults to `true`; removes `/usr/share/dotnet`.
- `haskell`: defaults to `true`; removes `/opt/ghc` and `/usr/local/.ghcup`.
- `boost`: defaults to `true`; removes `/usr/local/share/boost`.
- `swift`: defaults to `true`; removes `/usr/share/swift`.
- Output `cleaned`: `true` when hosted cleanup ran, `false` when skipped.

All inputs accept only `true` or `false`.
The action reports selected directories and disk usage before and after cleanup.
It preserves Docker data, Nix, the runner tool cache, Node and LLVM.
It does not accept arbitrary paths, remove package wildcards or alter swap.
Run it before setup steps, and set any category your project needs to `"false"`.
The [Devenv CI](../../workflows/devenv-ci/README.md), [Devenv Update](../../workflows/devenv-update/README.md), [Trivy](../../workflows/trivy/README.md), [CodeQL](../../workflows/codeql/README.md#hosted-disk-cleanup), and [Cargo CRAP](../../workflows/cargo-crap/README.md#hosted-disk-cleanup) reusable workflows expose the same five `cleanup-*` booleans, all defaulting to `true` on Linux hosted runners so projects opt out only of the SDKs they need.

## Debug Logging

Set `debug: true` to enable verbose tool logs and phase diagnostics.
GitHub's **Re-run with debug logging** enables the same behaviour automatically.
The default is `false`; GitHub debug logging still takes precedence.
See [the debug contract](../../README.md#debug-logging) for details.
