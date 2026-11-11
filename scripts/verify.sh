#!/usr/bin/env bash
# Run the repository quality gates in parallel and print a one-line summary.
# Usage: scripts/verify.sh [--with-e2e]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

WITH_E2E=0
if [[ "${1:-}" == "--with-e2e" ]]; then
  WITH_E2E=1
fi

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
run lint pnpm --dir web lint
run build pnpm --dir web build
if [[ "$WITH_E2E" -eq 1 ]]; then
  run e2e pnpm --dir web exec playwright test
fi

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
