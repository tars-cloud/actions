# setup-nix-cache

Configure optional Cachix access before setup-devenv constructs the consumer environment.
Supports GitHub.com Linux X64 and ARM64 runners 2.336.0+ with direct devenv or flake environments.
It does not require consumer manifests or enter a project shell.

Replace `<reviewed-sha>` with a revision containing this action.

```yaml
- id: setup_nix_cache
  name: Configure Nix binary caching
  uses: tars-cloud/actions/composite/setup-nix-cache@<reviewed-sha>
  with:
    cachix-name: ${{ vars.CACHIX_NAME }}
    cachix-token: ${{ secrets.CACHIX_TOKEN }}
```

Set `CACHIX_NAME` as a repository or organization variable and `CACHIX_TOKEN` as a secret, or pass a literal cache name.
The calling workflow must map them to inputs; the action does not infer a cache name or write token from the runner environment.
Unset variables and secrets resolve to empty inputs.

## Inputs and modes

- `cachix-name`: empty by default; no name disables Cachix even if a token is supplied.
- `cachix-token`: empty by default; a named cache without a token is read-only.
- `cachix-skip-push`: `false` by default; `true` permits authenticated reads but disables uploads.
- `cachix-push-filter`: empty by default; a regular expression excluding selected paths from uploads.
- A name and token enable writes except on fork PRs or when `cachix-skip-push` is true.
- Output `cachix-mode`: `disabled`, `read` or `write`.

Disabled mode skips Nix and Cachix setup entirely and leaves existing substituters untouched.
A named private cache needs authentication; token-free reads are intended for public caches.
Invalid names and failures in explicitly enabled Cachix setup fail the action.

To exclude source and archive paths, pass a filter such as `'(-source$|nixpkgs\.tar\.gz$|\.zip$)'`.
The pinned upstream integration embeds filters in shell code.
The action therefore accepts only valid single-word regexes without double quotes, backticks, whitespace, shell expansions, or escapes that shell quoting would change.
Dollar signs are supported as anchors at the end of the filter or before `)` and `|`.
Invalid filters fail before Nix or Cachix setup; unconfigured Cachix still skips everything.
Exclusions reduce uploads but do not prevent an excluded path from being uploaded as a dependency of another output.
See the [upstream filter contract](https://github.com/cachix/cachix-action/blob/38b082610b782e7e93e209c35fd730d399dee866/action.yml).

## Runtime and trust

When enabled, the action invokes setup-nix at the same action revision.
It reuses an installed Cachix CLI or installs it with `nix profile add nixpkgs#cachix`.
The pinned Cachix integration adds the named substituter while preserving existing substituters.
Write mode uses its daemon and post-job integration for pushes.
The upstream daemon mode may fall back to scanning newly created store paths when daemon support or trusted-user permissions are unavailable.
Write-enabled runners must be dedicated to the intended trust domain.
Cachix push behaviour is separate from setup-cache's success-only dependency archives.

Fork identity compares the PR head repository with the consuming repository, including PR-bearing events such as pull_request_target.
Fork jobs never receive the supplied token in the Cachix integration and always disable pushes.
Consumers must still withhold private credentials from untrusted jobs.
This policy does not make checking out untrusted code in a privileged workflow safe.

Existing system substituters, including Nginx mirrors, continue to work without this action.
No Attic integration, S3 Nix binary-cache backend or Nix-store archive is included.
Use [setup-cache](../setup-cache/README.md) separately for language and tool caches.

## Moving from setup-cache

Move `cachix-name` and `cachix-token` from setup-cache to this action and read `cachix-mode` from this step instead.
These inputs and the output have been removed from setup-cache.
Call setup-nix-cache before setup-devenv; setup-cache remains independently callable.
When using this action for Cachix pushes, avoid enabling a second publisher through the consumer's devenv configuration.
