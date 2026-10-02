#!/usr/bin/env bash
# Smoke test for a built mdvu binary: it starts and renders a small document.
# Run by release.yml on every native release binary and by ci.yml on every
# pull request, so a flag change that breaks the release fails here first.
set -euo pipefail

bin="${1:?usage: smoke-test.sh <path-to-mdvu>}"

"$bin" --version
printf '# Hello\n\n- item\n' | "$bin" --paging never --color never --width 40 -
