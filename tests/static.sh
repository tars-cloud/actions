#!/usr/bin/env bash
set -euo pipefail
# Use the same tools and configuration as devenv test and the commit hooks.
prek run --all-files
