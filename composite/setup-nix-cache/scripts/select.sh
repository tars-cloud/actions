#!/usr/bin/env bash
set -euo pipefail

mode=disabled
authenticated=false
if [[ -n ${CACHIX_NAME:-} ]]; then
	if [[ ! $CACHIX_NAME =~ ^[a-z0-9][a-z0-9-]*$ ]]; then
		echo '::error::Invalid cachix-name.'
		exit 1
	fi
	case ${CACHIX_SKIP_PUSH:-false} in
	true | false) ;;
	*)
		echo '::error::cachix-skip-push must be true or false.'
		exit 1
		;;
	esac
	# Upstream embeds this regex in double-quoted shell source and expands it unquoted.
	filter=${CACHIX_PUSH_FILTER:-}
	invalid=false
	for ((index = 0; index < ${#filter}; index++)); do
		character=${filter:index:1}
		next=${filter:index+1:1}
		case $character in
		'"' | '`' | [[:space:]]) invalid=true ;;
		'$') [[ -z $next || $next == ')' || $next == '|' ]] || invalid=true ;;
		\\)
			case $next in
			'' | \\ | '"' | '$' | '`' | [[:space:]]) invalid=true ;;
			esac
			;;
		esac
	done
	regex_status=0
	# shellcheck disable=SC2319
	[[ '' =~ $filter ]] || regex_status=$?
	if [[ ${invalid:-false} == true || $regex_status == 2 ]]; then
		echo '::error::cachix-push-filter must be a valid single-word regex without shell expansions, quotes, or unsafe escapes.'
		exit 1
	fi
	mode="read"
	if [[ -n ${CACHIX_TOKEN:-} && -n ${REPOSITORY:-} &&
		(-z ${HEAD_REPOSITORY:-} || ${HEAD_REPOSITORY,,} == "${REPOSITORY,,}") ]]; then
		authenticated=true
		if [[ ${CACHIX_SKIP_PUSH:-false} == false ]]; then
			mode="write"
		fi
	fi
fi
printf 'cachix-mode=%s\n' "$mode" >>"$GITHUB_OUTPUT"
printf 'cachix-authenticated=%s\n' "$authenticated" >>"$GITHUB_OUTPUT"
printf 'Cachix: %s.\n' "$mode"
