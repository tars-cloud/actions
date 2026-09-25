# free-disk-space

[Copyable workflow example](example.yaml).

Explicit cleanup for disposable GitHub-hosted Linux X64 and ARM64 runners 2.336.0+. Self-hosted runners always skip with
a notice; there is no override. Unsupported operating systems and architectures fail before cleanup.

```yaml
- uses: tars-cloud/actions/composite/free-disk-space@v1
  with:
    android: "true"
    dotnet: "true"
    haskell: "false"
    boost: "false"
    swift: "false"
```

- `android`: defaults to `true`; removes `/usr/local/lib/android`.
- `dotnet`: defaults to `true`; removes `/usr/share/dotnet`.
- `haskell`: defaults to `false`; removes `/opt/ghc` and `/usr/local/.ghcup`.
- `boost`: defaults to `false`; removes `/usr/local/share/boost`.
- `swift`: defaults to `false`; removes `/usr/share/swift`.
- Output `cleaned`: `true` when hosted cleanup ran, `false` when skipped.

All inputs accept only `true` or `false`. The action reports selected directories and disk usage before and after
cleanup. It preserves Docker data, Nix, the runner tool cache, Node and LLVM. It does not accept arbitrary paths, remove
package wildcards or alter swap. Run it before setup steps, and disable categories your project needs. No other public
action invokes cleanup automatically.
