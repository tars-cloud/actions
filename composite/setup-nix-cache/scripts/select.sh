#!/usr/bin/env bash
set -euo pipefail

mode=disabled
if [[ -n ${CACHIX_NAME:-} ]]; then
	if [[ ! $CACHIX_NAME =~ ^[a-z0-9][a-z0-9-]*$ ]]; then
		echo '::error::Invalid cachix-name.'
		exit 1
	fi
	mode="read"
	if [[ -n ${CACHIX_TOKEN:-} && -n ${REPOSITORY:-} &&
		(-z ${HEAD_REPOSITORY:-} || ${HEAD_REPOSITORY,,} == "${REPOSITORY,,}") ]]; then
		mode="write"
	fi
fi
printf 'cachix-mode=%s\n' "$mode" >>"$GITHUB_OUTPUT"
printf 'Cachix: %s.\n' "$mode"
