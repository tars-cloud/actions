#!/usr/bin/env bash
set -euo pipefail
case "${1:-}:${2:-}" in
x86_64-linux:x86_64-unknown-linux-gnu | x86_64-linux:x86_64-unknown-linux-musl | aarch64-linux:aarch64-unknown-linux-gnu | aarch64-linux:aarch64-unknown-linux-musl) ;;
*)
	echo '::error::Rust compiler host must match the selected Linux environment architecture.'
	exit 1
	;;
esac
