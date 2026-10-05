#!/usr/bin/env bash
set -euo pipefail
script_directory=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "$script_directory/environment.sh"
probe_system
system=${ENVIRONMENT_SYSTEM:-}
if [[ -z $system ]]; then
	case $RUNNER_ARCH in
	X64) system=x86_64-linux ;;
	ARM64) system=aarch64-linux ;;
	esac
fi
export TARS_PATH_BOUNDARY="${RUNNER_TEMP:-/tmp}/tars-path-boundary-$$-$RANDOM"
export PATH="$TARS_PATH_BOUNDARY:$PATH"
(
	# Compiler and LLVM locations must be supplied by the selected environment.
	unset RUSTUP_TOOLCHAIN RUSTC LLVM_COV LLVM_PROFDATA
	dispatch "$script_directory/analyze.sh" "$system"
)
