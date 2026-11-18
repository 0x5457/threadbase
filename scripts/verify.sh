#!/usr/bin/env bash
# Runs the quality gates in parallel: fmt, clippy, tests, types, React Doctor,
# build. The audit scripts under web/audit stay manual.
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
run doctor pnpm --dir web doctor:ci
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
