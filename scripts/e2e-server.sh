#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HOME_DIR="${TMPDIR:-/tmp}/threadbase-e2e"
DATA_DIR="$HOME_DIR/data"
REPO_DIR="$HOME_DIR/repo"

rm -rf "$HOME_DIR"
mkdir -p "$DATA_DIR" "$REPO_DIR"

git -C "$REPO_DIR" init -b main >/dev/null
git -C "$REPO_DIR" config user.email "e2e@example.com"
git -C "$REPO_DIR" config user.name "E2E"
printf 'one\ntwo\n' >"$REPO_DIR/README.md"
printf 'export const value = 1;\n' >"$REPO_DIR/app.ts"
git -C "$REPO_DIR" add -A
git -C "$REPO_DIR" commit -m "initial" >/dev/null

cd "$ROOT"
pnpm --dir web build >/dev/null
cargo build -p threadbase-agent --bin threadbase_acp_fixture >/dev/null
cargo build -p threadbase >/dev/null

export THREADBASE_DATA_DIR="$DATA_DIR"
export THREADBASE_BIND="127.0.0.1:7790"
export THREADBASE_ACP_COMMAND="$ROOT/target/debug/threadbase_acp_fixture"
export FAKE_AGENT_JOURNAL="$HOME_DIR/journal.log"
export RUST_LOG="${RUST_LOG:-threadbase=warn}"
export RUST_BACKTRACE=1
exec "$ROOT/target/debug/threadbase"
