#!/usr/bin/env bash
# Run the repository quality gates in parallel and print a one-line summary.
# Usage: scripts/verify.sh
#
# The frontend gate is `tsc --noEmit && vite build` (`pnpm build` already runs
# both). eslint and the Playwright suite were dropped with the v2 frontend, so
# there is no lint or e2e target to call here; the audit scripts under web/audit
# stay manual.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

pids=()
names=()
fail=0

run() {
  local name="$1"
  shift
  "$@" >"/tmp/threadbase-verify-${name}.log" 2>&1 &
  pids+=("$!")
  names+=("$name")
}

run fmt cargo fmt --all --check
run clippy env MBX_BLAZE=false cargo clippy --workspace --all-targets -- -D warnings
run nextest env MBX_BLAZE=false cargo nextest run --workspace
run typecheck pnpm --dir web typecheck
run build pnpm --dir web build

for index in "${!pids[@]}"; do
  if wait "${pids[$index]}"; then
    echo "PASS ${names[$index]}"
  else
    echo "FAIL ${names[$index]} (see /tmp/threadbase-verify-${names[$index]}.log)"
    fail=1
  fi
done

if [[ "$fail" -ne 0 ]]; then
  echo "verify: FAIL"
  exit 1
fi
echo "verify: PASS"
