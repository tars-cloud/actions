#!/usr/bin/env bash
set -euo pipefail

# Only Bash builtins run before publication, so broken project setup cannot block the summary.
declare -A results=() allowed=()
declare -a names=()
success=0 failure=0 cancelled=0 skipped=0 required_skipped=0
invalid=false
reporting_result=success
publication_result=skipped

input_error() {
	invalid=true
	printf '::error::%s\n' "$1" >&2
}

report_error() {
	reporting_result=failure
	printf '::warning::%s\n' "$1" >&2
}

trim() {
	REPLY=${1#"${1%%[![:space:]]*}"}
	REPLY=${REPLY%"${REPLY##*[![:space:]]}"}
}

for option in REPORT_FAIL_ON_ERROR REPORT_PUBLISH; do
	case ${!option:-false} in
	true | false) ;;
	*) input_error "$option must be true or false." ;;
	esac
done
case ${REPORT_ERRORS:-warn} in
warn | fail) ;;
*) input_error 'reporting-errors must be warn or fail.' ;;
esac

while IFS= read -r line || [[ -n $line ]]; do
	trim "$line"
	line=$REPLY
	[[ -n $line ]] || continue
	if [[ $line != *=* ]]; then
		input_error 'Each result must be name=result.'
		continue
	fi
	trim "${line%%=*}"
	name=$REPLY
	trim "${line#*=}"
	value=$REPLY
	if [[ ! $name =~ ^[a-zA-Z0-9_][a-zA-Z0-9_.-]*$ ]]; then
		input_error 'Check names must contain only letters, digits, underscores, dots or hyphens.'
		continue
	fi
	if [[ -v results[$name] ]]; then
		input_error 'Duplicate check name.'
		continue
	fi
	case $value in
	success) success=$((success + 1)) ;;
	failure) failure=$((failure + 1)) ;;
	cancelled) cancelled=$((cancelled + 1)) ;;
	skipped) skipped=$((skipped + 1)) ;;
	*)
		input_error 'Each result must be success, failure, cancelled or skipped.'
		value=unknown
		;;
	esac
	names+=("$name")
	results[$name]=$value
done <<<"${REPORT_RESULTS:-}"
if ((${#names[@]} == 0)); then
	input_error 'At least one named result is required.'
fi

while IFS= read -r line || [[ -n $line ]]; do
	trim "$line"
	name=$REPLY
	[[ -n $name ]] || continue
	if [[ ! $name =~ ^[a-zA-Z0-9_][a-zA-Z0-9_.-]*$ ]] || [[ ! -v results[$name] ]]; then
		input_error 'allow-skipped must name an existing check.'
		continue
	fi
	allowed[$name]=true
done <<<"${REPORT_ALLOW_SKIPPED:-}"
for name in "${names[@]}"; do
	if [[ ${results[$name]} == skipped && ! -v allowed[$name] ]]; then
		required_skipped=$((required_skipped + 1))
	fi
done

passed=false
if [[ $invalid == true ]]; then
	result=unknown
	reporting_result=failure
elif ((failure > 0)); then
	result=failure
elif ((cancelled > 0)); then
	result=cancelled
elif ((required_skipped > 0 || success == 0)); then
	result=skipped
else
	result=success
	passed=true
fi

# Escape user text for Markdown without evaluating it or invoking a formatter.
markdown() {
	local text=$1 character
	text=${text//&/\&amp;}
	text=${text//</\&lt;}
	text=${text//>/\&gt;}
	text=${text//$'\r'/ }
	text=${text//$'\n'/ }
	for character in $'\x5c' '`' '*' '_' '{' '}' '[' ']' '(' ')' '#' '+' '-' '!' '|'; do
		text=${text//"$character"/"\\$character"}
	done
	printf '%s' "$text"
}

summary() {
	printf '## '
	markdown "${REPORT_TITLE:-Pipeline results}"
	printf '\n\nResult: **%s**. Gate passed: **%s**.\n\n' "$result" "$passed"
	printf 'Success: %s; failure: %s; cancelled: %s; skipped: %s.\n\n' "$success" "$failure" "$cancelled" "$skipped"
	for name in "${names[@]}"; do
		printf -- '- \x60%s\x60: %s' "$name" "${results[$name]}"
		if [[ ${results[$name]} == skipped && -v allowed[$name] ]]; then
			printf ' (allowed)'
		fi
		printf '\n'
	done
	if [[ $invalid == true ]]; then
		printf '\nInvalid report inputs; see the action log.\n'
	fi
	if [[ -n ${REPORT_RUN_URL:-} ]]; then
		printf '\nRun: '
		markdown "$REPORT_RUN_URL"
		printf '\n'
	fi
	printf '\n'
}

if [[ -z ${GITHUB_STEP_SUMMARY:-} ]] || ! summary >>"$GITHUB_STEP_SUMMARY"; then
	report_error 'Could not write the GitHub job summary.'
fi

if [[ ${REPORT_PUBLISH:-false} == true && $invalid == false ]]; then
	publication_result=failure
	if ! type gh >/dev/null 2>&1; then
		report_error 'gh is not available in PATH; commit status was not published.'
	elif [[ -z ${GH_TOKEN:-} ]]; then
		report_error 'github-token is required for commit status publication.'
	elif [[ ! ${REPORT_REPOSITORY:-} =~ ^[a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+$ || ! ${REPORT_SHA:-} =~ ^[a-fA-F0-9]{40}$ || -z ${REPORT_CONTEXT:-} ]]; then
		report_error 'Publication requires an owner/repo repository, full commit SHA and nonempty status-context.'
	else
		case $result in
		success) state=success ;;
		failure) state=failure ;;
		*) state=error ;;
		esac
		description="$success succeeded, $failure failed, $cancelled cancelled, $skipped skipped"
		if gh api --method POST "repos/$REPORT_REPOSITORY/statuses/$REPORT_SHA" \
			--raw-field "state=$state" \
			--raw-field "context=$REPORT_CONTEXT" \
			--raw-field "description=$description" \
			--raw-field "target_url=${REPORT_RUN_URL:-}" --silent >/dev/null 2>&1; then
			publication_result=success
		else
			report_error 'Commit status publication failed; check token permissions and GitHub availability.'
		fi
	fi
fi

outputs() {
	printf 'result=%s\npassed=%s\n' "$result" "$passed"
	printf 'success-count=%s\nfailure-count=%s\ncancelled-count=%s\nskipped-count=%s\n' "$success" "$failure" "$cancelled" "$skipped"
	printf 'reporting-result=%s\npublication-result=%s\n' "$reporting_result" "$publication_result"
}
if [[ -z ${GITHUB_OUTPUT:-} ]] || ! outputs >>"$GITHUB_OUTPUT"; then
	report_error 'Could not write GitHub action outputs.'
fi

if [[ $invalid == true || (${REPORT_FAIL_ON_ERROR:-false} == true && $passed == false) || (${REPORT_ERRORS:-warn} == fail && $reporting_result == failure) ]]; then
	exit 1
fi
