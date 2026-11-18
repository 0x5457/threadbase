#!/usr/bin/env bash
# Builds the working tree and installs it as the user service.
# The binary embeds web/dist, so the web bundle is rebuilt first.
#
#   scripts/deploy.sh                 build + install + restart
#   scripts/deploy.sh --no-restart    build + install only
#   scripts/deploy.sh --wait-idle     wait until no turn is running, then deploy
#   scripts/deploy.sh --stage         leave the build for the server to install
#   scripts/deploy.sh --rollback      restore the last backup
#   scripts/deploy.sh --dry-run       print the plan, change nothing
#
# Env: THREADBASE_UNIT, THREADBASE_BIN, THREADBASE_BACKUPS, THREADBASE_HEALTH_URL,
#      THREADBASE_DATA_DIR, THREADBASE_AUTO_DEPLOY_PATH, THREADBASE_KEEP
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

UNIT="${THREADBASE_UNIT:-threadbase}"
BIN="${THREADBASE_BIN:-$HOME/.local/bin/threadbase}"
BACKUPS="${THREADBASE_BACKUPS:-$HOME/.local/share/threadbase/backups}"
HEALTH_URL="${THREADBASE_HEALTH_URL:-http://127.0.0.1:7777/api/health}"
KEEP="${THREADBASE_KEEP:-5}"

MODE="deploy"
DRY_RUN=0
WAIT_IDLE=0
for arg in "$@"; do
  case "$arg" in
    --no-restart) MODE="install" ;;
    --wait-idle) WAIT_IDLE=1 ;;
    --stage) MODE="stage" ;;
    --rollback) MODE="rollback" ;;
    --dry-run) DRY_RUN=1 ;;
    -h|--help)
      sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "deploy: unknown argument $arg" >&2
      exit 2
      ;;
  esac
done

DATA_DIR="${THREADBASE_DATA_DIR:-$HOME/.local/share/threadbase}"
if [[ "$(uname -s)" == "Darwin" && -z "${THREADBASE_DATA_DIR:-}" ]]; then
  DATA_DIR="$HOME/Library/Application Support/threadbase"
fi
DB="$DATA_DIR/threadbase.db"
STAGED="${THREADBASE_AUTO_DEPLOY_PATH:-$DATA_DIR/incoming/threadbase}"

say() { printf '\033[1m==>\033[0m %s\n' "$*"; }
run() {
  if [[ "$DRY_RUN" -eq 1 ]]; then
    printf '    [dry-run] %s\n' "$*"
  else
    "$@"
  fi
}

# Refuse to deploy from a dirty tree unless the caller insists: a deploy is
# what makes "the latest version" concrete, and half-committed work is hard to
# reproduce later.
if [[ "$MODE" == "deploy" ]] && [[ "$DRY_RUN" -eq 0 ]]; then
  if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "deploy: working tree has uncommitted changes." >&2
    echo "        commit them, or set THREADBASE_ALLOW_DIRTY=1 to deploy anyway." >&2
    if [[ "${THREADBASE_ALLOW_DIRTY:-0}" != "1" ]]; then
      exit 1
    fi
    echo "deploy: THREADBASE_ALLOW_DIRTY=1, continuing with a dirty tree." >&2
  fi
fi

if [[ "$MODE" == "rollback" ]]; then
  latest="$(ls -1t "$BACKUPS"/threadbase-* 2>/dev/null | head -1 || true)"
  if [[ -z "$latest" ]]; then
    echo "deploy: no backups in $BACKUPS" >&2
    exit 1
  fi
  say "Rolling back to $(basename "$latest")"
  run install -m 755 "$latest" "$BIN"
  run "$BIN" service restart --unit "$UNIT"
  if [[ "$DRY_RUN" -eq 1 ]]; then
    say "Dry run: would roll back to $(basename "$latest")."
  else
    say "Rolled back. $(basename "$latest") is now installed."
  fi
  exit 0
fi

say "Target"
printf '    binary   %s\n' "$BIN"
printf '    unit     %s\n' "$UNIT"
printf '    backups  %s (keep %s)\n' "$BACKUPS" "$KEEP"

# 1. Web bundle. Must run first: `cargo build --release` embeds these bytes.
say "Building web bundle (pnpm --dir web build)"
run pnpm --dir web build
if [[ "$DRY_RUN" -eq 0 ]]; then
  if [[ ! -f web/dist/index.html ]]; then
    echo "deploy: web/dist/index.html missing after build" >&2
    exit 1
  fi
  asset="$(grep -oE 'assets/index-[^"]+\.js' web/dist/index.html | head -1)"
  printf '    bundled  %s\n' "$asset"
fi

# 2. Release binary (embeds the bundle above).
say "Building release binary (cargo build --release -p threadbase)"
run cargo build --release -p threadbase

NEW_BIN="target/release/threadbase"
if [[ "$DRY_RUN" -eq 0 ]]; then
  if [[ ! -x "$NEW_BIN" ]]; then
    echo "deploy: $NEW_BIN not found" >&2
    exit 1
  fi
  # The embedded asset must match what the web build produced, otherwise the
  # release profile silently shipped a stale UI.
  # `strings | grep -q` under `pipefail` yields a false negative: grep exits
  # early, strings gets SIGPIPE, the pipeline "fails". Count instead.
  embedded="$(strings "$NEW_BIN" | grep -F -c -- "$asset" || true)"
  if [[ "${embedded:-0}" -eq 0 ]]; then
    echo "deploy: release binary does not contain $asset; refusing to install." >&2
    exit 1
  fi
  printf '    embedded %s\n' "$asset"
fi

# 2b. Optionally wait for a moment when no turn is running.
if [[ "$WAIT_IDLE" -eq 1 && "$MODE" == "deploy" ]]; then
  if ! command -v sqlite3 >/dev/null 2>&1; then
    echo "deploy: --wait-idle needs sqlite3 to read $DB" >&2
    exit 1
  fi
  say "Waiting for an idle window (Ctrl-C to deploy anyway)"
  waited=0
  while :; do
    busy="$(sqlite3 "$DB" "SELECT
        (SELECT COUNT(*) FROM threads WHERE status IN ('preparing','running','requires_action'))
      + (SELECT COUNT(*) FROM approvals WHERE status='pending')
      + (SELECT COUNT(*) FROM elicitations WHERE status='pending')
      + (SELECT COUNT(*) FROM queued_inputs WHERE status='queued')
      + (SELECT COUNT(*) FROM subagents WHERE status IN ('running','requires_action'));" 2>/dev/null || echo 1)"
    [[ "${busy:-1}" == "0" ]] && break
    if [[ $((waited % 60)) -eq 0 ]]; then
      printf '    %ss: %s item(s) still busy\n' "$waited" "$busy"
    fi
    sleep 5
    waited=$((waited + 5))
    if [[ "$waited" -ge 7200 ]]; then
      echo "deploy: still busy after two hours; giving up" >&2
      exit 1
    fi
  done
  say "Idle now; deploying"
fi

# 2c. Hand the build to the server instead of restarting it ourselves.
if [[ "$MODE" == "stage" ]]; then
  say "Staging to $STAGED"
  run mkdir -p "$(dirname "$STAGED")"
  run install -m 755 "$NEW_BIN" "$STAGED"
  if [[ "$DRY_RUN" -eq 0 ]]; then
    printf '    staged   %s\n' "$STAGED"
    health="$(curl -fsS --max-time 3 "$HEALTH_URL" 2>/dev/null || true)"
    if [[ "$health" == *'"deployStaged":true'* ]]; then
      say "The server sees it; it will install at the next idle window."
    else
      say "Staged. Turn it on with THREADBASE_AUTO_DEPLOY=1 on the server,"
      say "or deploy normally: $0 --wait-idle"
    fi
  fi
  exit 0
fi

# 3. Back up the currently installed binary.
stamp="$(date +%Y%m%d-%H%M%S)"
if [[ -f "$BIN" ]]; then
  say "Backing up current binary"
  run mkdir -p "$BACKUPS"
  run cp -a "$BIN" "$BACKUPS/threadbase-$stamp"
  if [[ "$DRY_RUN" -eq 0 ]]; then
    printf '    saved    %s\n' "$BACKUPS/threadbase-$stamp"
  fi
fi

# 4. Install.
say "Installing to $BIN"
run install -m 755 "$NEW_BIN" "$BIN"

# 5. Prune old backups.
if [[ "$DRY_RUN" -eq 0 ]]; then
  mapfile -t old < <(ls -1t "$BACKUPS"/threadbase-* 2>/dev/null | tail -n +$((KEEP + 1)))
  for path in "${old[@]:-}"; do
    [[ -n "$path" ]] && rm -f "$path"
  done
fi

if [[ "$MODE" == "install" ]]; then
  say "Installed without restarting. Run: $BIN service restart --unit $UNIT"
  exit 0
fi

# 6. Restart and verify.
say "Restarting $UNIT"
run "$BIN" service restart --unit "$UNIT"

if [[ "$DRY_RUN" -eq 0 ]]; then
  say "Waiting for health"
  for attempt in $(seq 1 30); do
    if body="$(curl -fsS --max-time 2 "$HEALTH_URL" 2>/dev/null)"; then
      printf '    %s\n' "$body"
      break
    fi
    if [[ "$attempt" -eq 30 ]]; then
      echo "deploy: health check failed after 15s; last log lines:" >&2
      "$BIN" service logs --unit "$UNIT" --lines 50 >&2 || true
      exit 1
    fi
    sleep 0.5
  done

  served="$(curl -fsS --max-time 3 "${HEALTH_URL%/api/health}/" 2>/dev/null \
    | grep -oE 'assets/index-[^"]+\.js' | head -1 || true)"
  if [[ -n "$asset" && "$served" != "$asset" ]]; then
    echo "deploy: served asset '$served' != built asset '$asset'" >&2
    exit 1
  fi
  printf '    serving  %s\n' "${served:-unknown}"

  # The log scan that used to live here counted agent adapters tearing down
  # during the restart and failed deploys that were fine. `/api/health` plus the
  # served asset is the real gate; `threadbase service logs` is one command on
  # both platforms when a human needs to look.
  say "Deployed. Rollback with: $0 --rollback"
fi
