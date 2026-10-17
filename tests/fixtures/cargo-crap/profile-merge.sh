#!/usr/bin/env bash
set -euo pipefail
compiler=${RUSTC:-rustc}
sysroot=$("$compiler" --print sysroot)
host=$("$compiler" -vV)
host=${host##*$'\nhost: '}
host=${host%%$'\n'*}
profdata="$sysroot/lib/rustlib/$host/bin/llvm-profdata"

# Corrupt only the consumer's collected profiles, after tests exit and before the real merge.
if [[ ${1:-} == merge && -f .cargo-crap.toml ]]; then
	list=''
	for ((i = 1; i <= $#; i++)); do
		if [[ ${!i} == -f ]]; then
			((i += 1))
			list=${!i}
			break
		fi
	done
	[[ -n $list && -s $list ]]
	case ${CRAP_FIXTURE_PROFILES:?Profile fixture case is required} in
	mixed)
		invalid="${list%/*}/tact-invalid.profraw"
		printf 'invalid raw profile\n' >"$invalid"
		printf '%s\n' "$invalid" >>"$list"
		;;
	all-invalid)
		while IFS= read -r raw; do
			printf 'invalid raw profile\n' >"$raw"
		done <"$list"
		;;
	*) exit 1 ;;
	esac
fi
exec "$profdata" "$@"
