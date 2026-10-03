#!/usr/bin/env bash
set -euo pipefail
script_directory=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "$script_directory/environment.sh"
validate_environment
if foreign_system; then
	echo '::error::Cargo CRAP requires a native Linux AMD64 or ARM64 environment.'
	exit 1
fi
export TARS_PATH_BOUNDARY="${RUNNER_TEMP:-/tmp}/tars-path-boundary-$$-$RANDOM"
export PATH="$TARS_PATH_BOUNDARY:$PATH"
(
	# Compiler and LLVM locations must be supplied by the selected environment.
	unset RUSTUP_TOOLCHAIN RUSTC LLVM_COV LLVM_PROFDATA
	dispatch "$script_directory/analyze.sh"
)
