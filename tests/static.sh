#!/usr/bin/env bash
set -euo pipefail
mapfile -t scripts < <(rg --files internal tests -g '*.sh')
shellcheck -x "${scripts[@]}"
shfmt -d "${scripts[@]}"
mapfile -t metadata < <(rg --files --hidden setup-* free-disk-space internal .github tests -g '*.yml' -g '*.yaml')
yamllint "${metadata[@]}"
actionlint
mapfile -t markdown < <(rg --files -g '*.md' -g '!.tars/**')
markdownlint "${markdown[@]}"
