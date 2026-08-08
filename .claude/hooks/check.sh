#!/usr/bin/env bash
# Stop: Rust の変更が残っていれば clippy と未レビュー snapshot を検査し、
# 落ちていれば exit 2 で差し戻す。
#
# 同じ内容で二度は差し戻さない。Stop hook の exit 2 は応答終了を止めるため、
# 直せない失敗（環境起因など）で無限ループになるのを防ぐ。
set -uo pipefail

input=$(cat)
cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0

# 未コミットの Rust 変更が無ければ何も検査しない。
[[ -n $(git status --porcelain -- '*.rs' Cargo.toml Cargo.lock 2>/dev/null) ]] || exit 0

report=""

if ! clippy=$(cargo clippy --all-targets --all-features --message-format short -- -D warnings 2>&1); then
  report+="cargo clippy --all-targets --all-features -- -D warnings が失敗した:"$'\n'
  report+=$(printf '%s\n' "$clippy" | grep -E '^(error|warning)' | head -30)
  report+=$'\n'
fi

pending=$(find tests -name '*.pending-snap' 2>/dev/null)
if [[ -n $pending ]]; then
  report+="未レビューの snapshot が残っている。cargo insta review で確認すること:"$'\n'
  report+="$pending"$'\n'
fi

[[ -n $report ]] || exit 0

session=$(jq -r '.session_id // "none"' <<<"$input")
stamp="${TMPDIR:-/tmp}/mdvu-stop-hook-${session}"
signature=$(printf '%s' "$report" | shasum -a 256 | cut -d' ' -f1)
[[ -f $stamp && $(cat "$stamp") == "$signature" ]] && exit 0
printf '%s' "$signature" >"$stamp"

printf '%s' "$report" >&2
exit 2
