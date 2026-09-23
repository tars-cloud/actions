#!/usr/bin/env bash
set -euo pipefail

if [[ ${RUNNER_OS:-} != Linux || ! ${RUNNER_ARCH:-} =~ ^(X64|ARM64)$ ]]; then
	echo '::error::Supported platforms are Linux X64 and ARM64.'
	exit 1
fi
case ${RUNNER_ENVIRONMENT:-} in
github-hosted | self-hosted) ;;
*)
	echo '::error::Unknown runner.environment; expected github-hosted or self-hosted.'
	exit 1
	;;
esac
