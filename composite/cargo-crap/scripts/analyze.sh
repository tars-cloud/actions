#!/usr/bin/env bash
set -euo pipefail
if [[ -n ${TARS_PATH_BOUNDARY:-} && $PATH == *":$TARS_PATH_BOUNDARY:"* ]]; then
	tool_path=${PATH%%":$TARS_PATH_BOUNDARY:"*}
else
	tool_path=${DEVENV_PROFILE:+$DEVENV_PROFILE/bin}
fi
missing=0
case ${CRAP_COVERAGE_TOOL:-llvm-cov} in
llvm-cov | tarpaulin) ;;
*)
	echo '::error::coverage-tool must be llvm-cov or tarpaulin.'
	exit 1
	;;
esac
for tool in cargo rustc "cargo-${CRAP_COVERAGE_TOOL:-llvm-cov}" git sha256sum; do
	if [[ -z $tool_path ]] || ! PATH="$tool_path" command -v "$tool" >/dev/null; then
		printf '::error::Declare %s in the selected devenv packages or flake devShell and update its lockfile. See the Cargo CRAP setup documentation.\n' "$tool"
		missing=1
	fi
done
if ((missing)); then exit 1; fi
# Restrict execution too: Cargo resolves subcommands through PATH.
export PATH="$tool_path"
export RUSTUP_AUTO_INSTALL=0
compiler=$(rustc -vV)
rust_version=${compiler%%$'\n'*}
if [[ ! $rust_version =~ ^rustc\ ([0-9]+)\.([0-9]+)\. ||
	(${BASH_REMATCH[1]:-0} -eq 1 && ${BASH_REMATCH[2]:-0} -lt 88) ]]; then
	echo '::error::The bundled analysis helper requires Rust 1.88 or newer in the selected environment.'
	exit 1
fi
host=${compiler##*$'\nhost: '}
host=${host%%$'\n'*}
bash "$CRAP_ACTION_ROOT/scripts/compiler-host.sh" "${1:?Selected Nix system is required}" "$host"
sysroot=$(rustc --print sysroot)
# Tarpaulin turns RUSTUP_TOOLCHAIN into cargo +toolchain, which direct Nix Cargo does not support.
if cargo "+$sysroot" --version >/dev/null 2>&1; then
	export RUSTUP_TOOLCHAIN="$sysroot"
else
	unset RUSTUP_TOOLCHAIN
fi
if [[ ! -x $sysroot/bin/rustc ]]; then
	echo '::error::The selected Rust sysroot must contain its compiler binary.'
	exit 1
fi
export RUSTC="$sysroot/bin/rustc"
export LLVM_COV="${LLVM_COV:-$sysroot/lib/rustlib/$host/bin/llvm-cov}"
export LLVM_PROFDATA="${LLVM_PROFDATA:-$sysroot/lib/rustlib/$host/bin/llvm-profdata}"
for tool in "$LLVM_COV" "$LLVM_PROFDATA"; do
	if [[ $tool != /* || ! -x $tool ]]; then
		printf '::error::Declare compatible LLVM tools or add llvm-tools-preview to the selected Rust toolchain: %s\n' "$tool"
		missing=1
	fi
done
if ((missing)); then exit 1; fi
export CARGO_LLVM_COV_SETUP=no
consumer=$PWD
tool_root=$(cd "$CRAP_ACTION_ROOT/../.." && pwd)
tool_target="$RUNNER_TEMP/cargo-crap-helper"
(
	cd "$tool_root"
	CARGO_TARGET_DIR="$tool_target" cargo build --locked --package actions-crap --target "$host"
)
CRAP_TOOL_TARGET="$tool_target"
CRAP_HELPER="$tool_target/$host/debug/actions-crap"
CRAP_HOST="$host"
# shellcheck source=install-cargo-crap.sh
source "$CRAP_ACTION_ROOT/scripts/install-cargo-crap.sh"
smoke="$RUNNER_TEMP/cargo-crap-smoke-$$"
mkdir -p "$smoke"
cp -R "$CRAP_ACTION_ROOT/scripts/smoke/." "$smoke/"
(
	cd "$smoke"
	case ${CRAP_COVERAGE_TOOL:-llvm-cov} in
	llvm-cov) cargo llvm-cov --locked --lcov --output-path "$smoke/lcov.info" ;;
	tarpaulin) cargo tarpaulin --locked --ignore-config --engine Llvm --out Lcov --output-dir "$smoke" ;;
	esac
	cargo crap --path src --lcov "$smoke/lcov.info" --format json --output "$smoke/report.json"
)
"$tool_target/$host/debug/actions-crap" validate-smoke "$smoke/report.json"
cd "$consumer"
export CRAP_ACTION_REVISION
CRAP_ACTION_REVISION=$(
	for file in "$tool_root/crates/actions-crap/src/main.rs" "$tool_root/Cargo.lock" "$CRAP_ACTION_ROOT/scripts/analyze.sh" "$CRAP_ACTION_ROOT/scripts/compiler-host.sh" "$CRAP_ACTION_ROOT/scripts/install-cargo-crap.sh" "$CRAP_ACTION_ROOT/scripts/dispatch.sh" "$CRAP_ACTION_ROOT/scripts/environment.sh" "$CRAP_ACTION_ROOT/scripts/platform.sh" "$CRAP_ACTION_ROOT/scripts/schemas/report-v1.json" "$CRAP_ACTION_ROOT/scripts/schemas/delta-v2.json"; do
		fingerprint=$(sha256sum "$file")
		printf '%s' "${fingerprint%% *}"
	done | sha256sum
)
CRAP_ACTION_REVISION=${CRAP_ACTION_REVISION%% *}
"$tool_target/$host/debug/actions-crap"
