#!/usr/bin/env bash
# PostToolUse(Edit|Write): 編集した Rust ファイルだけを整形する。
# cargo fmt はクレート全体を書き換えるため、編集していないファイルまで差分に載る。
set -uo pipefail

file=$(jq -r '.tool_input.file_path // empty')
[[ $file == *.rs && -f $file ]] || exit 0

# 整形できない（構文エラーなど）ときは黙って通す。Stop hook の clippy が拾う。
rustfmt --edition 2024 "$file" >/dev/null 2>&1
exit 0
