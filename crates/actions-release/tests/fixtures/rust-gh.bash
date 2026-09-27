#!/usr/bin/env bash
set -euo pipefail

if [[ -f $RELEASE_FIXTURE/fail-api ]]; then
	printf 'fixture transport failure\n' >&2
	exit 1
fi
kind=$1
shift
if [[ $kind == run ]]; then
	[[ $1 == download ]]
	shift 2
	artifact='' destination=''
	while (($#)); do
		case $1 in
		--repo) shift 2 ;;
		--name)
			artifact=$2
			shift 2
			;;
		--dir)
			destination=$2
			shift 2
			;;
		*) exit 2 ;;
		esac
	done
	mkdir -p "$destination"
	cp -R "$RELEASE_FIXTURE/artifacts/$artifact/." "$destination/"
	exit
fi
if [[ $kind == release ]]; then
	[[ $1 == upload ]]
	shift 2
	file=
	while (($#)); do
		case $1 in
		--repo) shift 2 ;;
		--clobber) shift ;;
		*)
			file=$1
			shift
			;;
		esac
	done
	name=${file##*/}
	if [[ -f $RELEASE_FIXTURE/fail-upload ]]; then
		jq --arg name "$name" 'map(select(.name != $name)) + [{id: 55, name: $name, digest: null, state: "starter"}]' \
			"$RELEASE_FIXTURE/assets.json" >"$RELEASE_FIXTURE/assets.next"
		mv "$RELEASE_FIXTURE/assets.next" "$RELEASE_FIXTURE/assets.json"
		printf 'fixture interrupted upload\n' >&2
		exit 1
	fi
	checksum=$(sha256sum "$file")
	checksum=${checksum%% *}
	jq --arg name "$name" --arg digest "sha256:$checksum" \
		'map(select(.name != $name)) + [{id: 55, name: $name, digest: $digest, state: "uploaded"}]' \
		"$RELEASE_FIXTURE/assets.json" >"$RELEASE_FIXTURE/assets.next"
	mv "$RELEASE_FIXTURE/assets.next" "$RELEASE_FIXTURE/assets.json"
	printf 'upload %s\n' "$name" >>"$RELEASE_FIXTURE/writes"
	exit
fi
[[ $kind == api ]]
method=GET route='' paginated=false
while (($#)); do
	case $1 in
	--method)
		method=$2
		shift 2
		;;
	--input) shift 2 ;;
	--paginate | --slurp)
		paginated=true
		shift
		;;
	repos/owner/consumer)
		route=''
		shift
		;;
	repos/owner/consumer/)
		printf 'gh: Not Found (HTTP 404)\n' >&2
		exit 1
		;;
	repos/owner/consumer/*)
		route=${1#repos/owner/consumer/}
		shift
		;;
	*) exit 2 ;;
	esac
done
if [[ $method == GET ]]; then
	case $route in
	"") file=repository.json ;;
	pulls\?state=open*) file=open.json ;;
	pulls\?state=closed*) file=closed.json ;;
	commits/*/pulls*)
		sha=${route#commits/}
		sha=${sha%%/*}
		file="commit-$sha.json"
		if [[ ! -f $RELEASE_FIXTURE/$file ]]; then printf '[]' >"$RELEASE_FIXTURE/$file"; fi
		;;
	releases\?*) file=releases.json ;;
	releases/9/assets*) file=assets.json ;;
	actions/runs/77) file=run.json ;;
	actions/runs/77/artifacts*) file=artifacts.json ;;
	*)
		printf 'Unexpected GET: %s\n' "$route" >&2
		exit 2
		;;
	esac
	if [[ $paginated == true ]]; then jq -s '.' "$RELEASE_FIXTURE/$file"; else cat "$RELEASE_FIXTURE/$file"; fi
	exit
fi
cat >"$RELEASE_FIXTURE/payload.json"
printf '%s %s\n' "$method" "$route" >>"$RELEASE_FIXTURE/writes"
case $route in
pulls | pulls/42)
	branch=$(jq -r .default_branch "$RELEASE_FIXTURE/repository.json")
	head=$(git --git-dir="$RELEASE_REMOTE" rev-parse refs/heads/release/next)
	base=$(git --git-dir="$RELEASE_REMOTE" rev-parse "refs/heads/$branch")
	jq --arg head "$head" --arg base "$base" --arg branch "$branch" \
		'. + {number:42, html_url:"https://example.invalid/pull/42", merged_at:null,
        head:{sha:$head, ref:"release/next", repo:{full_name:"owner/consumer"}},
        base:{sha:$base, ref:$branch, repo:{full_name:"owner/consumer"}}}' \
		"$RELEASE_FIXTURE/payload.json" >"$RELEASE_FIXTURE/pr.json"
	jq -s '.' "$RELEASE_FIXTURE/pr.json" >"$RELEASE_FIXTURE/open.json"
	cat "$RELEASE_FIXTURE/pr.json"
	;;
releases)
	jq '. + {id:9, html_url:"https://example.invalid/releases/9"}' \
		"$RELEASE_FIXTURE/payload.json" >"$RELEASE_FIXTURE/release.json"
	jq -s '.' "$RELEASE_FIXTURE/release.json" >"$RELEASE_FIXTURE/releases.json"
	cat "$RELEASE_FIXTURE/release.json"
	;;
releases/9)
	jq -s '.[0] * .[1]' "$RELEASE_FIXTURE/release.json" "$RELEASE_FIXTURE/payload.json" >"$RELEASE_FIXTURE/release.next"
	mv "$RELEASE_FIXTURE/release.next" "$RELEASE_FIXTURE/release.json"
	tag=$(jq -r .tag_name "$RELEASE_FIXTURE/release.json")
	commit=$(jq -r .target_commitish "$RELEASE_FIXTURE/release.json")
	git --git-dir="$RELEASE_REMOTE" update-ref "refs/tags/$tag" "$commit"
	jq -s '.' "$RELEASE_FIXTURE/release.json" >"$RELEASE_FIXTURE/releases.json"
	cat "$RELEASE_FIXTURE/release.json"
	;;
*)
	printf 'Unexpected write: %s %s\n' "$method" "$route" >&2
	exit 2
	;;
esac
