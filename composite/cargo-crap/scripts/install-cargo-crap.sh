#!/usr/bin/env bash
set -euo pipefail
if ! command -v cargo-crap >/dev/null 2>&1; then
	mkdir -p "$CRAP_TOOL_TARGET"
	metadata="$CRAP_TOOL_TARGET/tool-metadata.json"
	if ! cargo metadata --locked --all-features --format-version 1 >"$metadata"; then
		echo '::error::Resolve the consumer Cargo.toml dependencies and commit an up-to-date Cargo.lock before installing cargo-crap.'
		exit 1
	fi
	tool_version=$("$CRAP_HELPER" resolve-tool "$metadata")
	CARGO_TARGET_DIR="$CRAP_TOOL_TARGET/install-target" cargo install --locked --version "$tool_version" --registry crates-io --root "$CRAP_TOOL_TARGET" --target "$CRAP_HOST" --bin cargo-crap cargo-crap
	export PATH="$CRAP_TOOL_TARGET/bin:$PATH"
fi
version=$(cargo crap --version)
if [[ $version != 'cargo-crap 0.6.1' ]]; then
	echo '::error::Analysis contract v1 requires cargo-crap 0.6.1 from the selected environment or a locked Cargo dependency.'
	exit 1
fi
