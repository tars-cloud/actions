# Migrating to the next release

The examples target the upcoming `v1` alias.
After publication, pin all shared action calls to the same reviewed full commit SHA.
Before publication, replace `@v1` with the revision you are testing.
No release or consumer migration happens automatically.

## Split language caches from Nix caches

Keep Cargo, Python, Bun and Trivy archive inputs on `setup-cache`.
Move `cachix-name` and `cachix-token` to a separate `setup-nix-cache` call, and read `cachix-mode` from that new step.
Order the steps as checkout, setup-cache, setup-nix-cache, setup-devenv, then run-devenv or setup-trivy.
Both cache actions are optional.

Pass `vars.CACHIX_NAME` and `secrets.CACHIX_TOKEN` explicitly from the consumer.
A missing name disables Cachix; a name without a token enables public reads.
Use `cachix-skip-push: "true"` for authenticated reads without uploads.
Use `cachix-push-filter` to exclude matching store paths from upload selection.
The [Nix cache workflow](../composite/setup-nix-cache/example.yaml) shows these inputs together.

System-configured Nix substituters remain usable by devenv through Nix.
No Attic service or Nix-store archive is introduced.

## Replace local shell wrappers

Use `setup-devenv` to prepare the environment, then `run-devenv` for consumer commands.
Ordinary workflow `run` steps do not automatically enter the prepared shell.
Pass matching `type`, `working-directory`, `flake-shell` and `system` inputs to each action that enters the environment.
Pass the same environment selection and system to `setup-cache` for compatible compiled-cache keys.

The [setup-devenv workflow](../composite/setup-devenv/example.yaml) covers native X64 and ARM64, a pinned CLI, setup outputs, and optional preconfigured emulation.
An explicit `devenv-installable` overrides any installed CLI and applies only to direct mode.
Foreign-system execution requires existing runner emulation and Nix `extra-platforms`; the action validates both configuration and shell execution.
The [run-devenv workflow](../composite/run-devenv/example.yaml) covers a flake environment.
Replace its sample command with your project's command.

For private dependency inputs, pass the same `github-token` to setup-devenv, run-devenv and setup-trivy.
The [Trivy workflow](../composite/setup-trivy/example.yaml) demonstrates this and requires Trivy declared in the selected environment.

## Inspect cache restoration

The [cache workflow](../composite/setup-cache/example.yaml) covers GitHub-hosted runners and an optional self-hosted S3 job.
Supply all required S3 inputs on self-hosted runners; absent configuration uses GitHub storage, while partial configuration fails clearly.
Use the per-tool `*-status` outputs to distinguish exact hits, compatible fallbacks, reported errors and skipped restores.
`miss-or-unavailable` preserves the upstream ambiguity between a cold miss and some recoverable transport failures.
Normal misses are notices; reported failures are warnings.
The log includes the requested key, paths and save policy.
Confirm actual uploads in the backend post-job save log after a successful job that populated the cache paths.

## Keep examples valid

Every action has an `example.yaml`, including private helpers whose examples use their public parent action.
CI validates workflow syntax, action pins, input names, required inputs and referenced shared-action outputs.
Optional self-hosted examples require a repository variable and a manual dispatch; adjust runner labels for your installation.
