#!/usr/bin/env bash
set -euo pipefail
paths=("$CARGO_HOME/registry/index" "$CARGO_HOME/registry/cache" "$CARGO_HOME/git/db" "$CARGO_TARGET_DIR" "$UV_CACHE_DIR" "$PIP_CACHE_DIR" "$BUN_INSTALL_CACHE_DIR" "$TRIVY_CACHE_DIR")
for directory in "${paths[@]}"; do
	case $1 in
	seed)
		mkdir -p "$directory"
		printf '%s\n' "$GITHUB_RUN_ID" >"$directory/tars-cache-proof"
		;;
	verify)
		[[ $(cat "$directory/tars-cache-proof") == "$GITHUB_RUN_ID" ]]
		;;
	*) exit 1 ;;
	esac
done
