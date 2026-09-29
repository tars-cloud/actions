#!/usr/bin/env bash
set -euo pipefail

case "${RELEASE_COMMAND:-}" in
candidate | prepare | publish) ;;
*)
	printf '::error::command must be candidate, prepare or publish.\n'
	exit 1
	;;
esac
if [[ ! ${RELEASE_COMMIT:-} =~ ^[a-fA-F0-9]{40}$ ]]; then
	printf '::error::commit-sha must be a full 40-character commit SHA.\n'
	exit 1
fi
# Clean devenv shells discard the runner PATH; their profile lists declared tools.
if [[ -n ${RELEASE_PATH_BOUNDARY:-} && $PATH == *":$RELEASE_PATH_BOUNDARY:"* ]]; then
	tool_path=${PATH%%":$RELEASE_PATH_BOUNDARY:"*}
else
	tool_path=${DEVENV_PROFILE:+$DEVENV_PROFILE/bin}
	if [[ -n $tool_path ]]; then export PATH="$tool_path:$PATH"; fi
fi
for tool in cargo rustc convco git gh sha256sum; do
	if [[ -z $tool_path ]] || ! PATH="$tool_path" command -v "$tool" >/dev/null; then
		printf '::error::Add %s to the consumer devenv environment and update its lockfile.\n' "$tool"
		exit 1
	fi
done
host=$(rustc -vV)
version=${host%%$'\n'*}
if [[ ! $version =~ ^rustc\ ([0-9]+)\.([0-9]+)\. ||
	(${BASH_REMATCH[1]:-0} -eq 1 && ${BASH_REMATCH[2]:-0} -lt 88) ]]; then
	printf '::error::The release tool requires Rust 1.88 or newer in the consumer environment.\n'
	exit 1
fi
host=${host##*$'\nhost: '}
host=${host%%$'\n'*}
if [[ ! $host =~ ^(x86_64|aarch64)-unknown-linux-(gnu|musl)$ ]]; then
	printf '::error::The release tool requires a native Linux Rust compiler.\n'
	exit 1
fi
consumer=$PWD
tool_root=$(cd "$RELEASE_ACTION_ROOT/../.." && pwd)
tool_target="$RUNNER_TEMP/release-rust-tool"
# Build the bundled revision independently of consumer Cargo configuration and output caches.
(
	cd "$tool_root"
	CARGO_TARGET_DIR="$tool_target" cargo build --locked --package actions-release --target "$host"
)
args=(rust "$RELEASE_COMMAND" --commit "$RELEASE_COMMIT")
if [[ $RELEASE_COMMAND == publish ]]; then
	args+=(--artifact-run-id "$RELEASE_ARTIFACT_RUN" --manifest-artifact "$RELEASE_MANIFEST")
fi
cd "$consumer"
"$tool_target/$host/debug/actions-release" "${args[@]}"
