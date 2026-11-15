#!/usr/bin/env bash
# Code-health measurement: objective signals only, no rewriting.
#
#   scripts/quality/measure.sh          all three layers
#   scripts/quality/measure.sh dup      clone detection (jscpd)
#   scripts/quality/measure.sh dead     dead code (knip)
#   scripts/quality/measure.sh complex  cyclomatic complexity (eslint)
#
# Reports land in .quality/ (gitignored) for the review pass in review.md.
#
# A note on the complexity layer: the repo deliberately dropped eslint with the
# v2 frontend (see scripts/verify.sh), and `typescript@7` ships no JavaScript
# compiler API, so a hand-rolled AST script is not an option either. eslint and
# typescript-eslint are therefore installed into a cache directory outside the
# repository and never appear in web/package.json or pnpm-lock.yaml.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WEB="$ROOT/web"
OUT="$ROOT/.quality"
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/threadbase-quality"
mkdir -p "$OUT"

layer="${1:-all}"
status_total=0

header() { printf '\n== %s ==\n' "$1"; }

run_dup() {
  header "duplication (jscpd)"
  local code=0
  (cd "$WEB" && pnpm dlx jscpd@4 src) || code=$?
  # jscpd exits 1 only when duplication crosses .jscpd.json "threshold".
  printf 'duplication: %s\n' "$([[ $code -eq 0 ]] && echo ok || echo over-threshold)"
  return "$code"
}

run_dead() {
  header "dead code (knip)"
  local code=0
  (cd "$WEB" && pnpm dlx knip@5 --reporter compact) || code=$?
  printf 'dead code: %s\n' "$([[ $code -eq 0 ]] && echo clean || echo findings)"
  return "$code"
}

run_complex() {
  header "complexity (eslint)"
  if [[ ! -x "$CACHE/node_modules/.bin/eslint" ]]; then
    printf 'installing eslint+typescript-eslint into %s\n' "$CACHE"
    if ! (mkdir -p "$CACHE" && cd "$CACHE" && npm i --silent --no-fund --no-audit eslint@9 typescript-eslint@8 >/dev/null 2>&1); then
      printf 'complexity: SKIPPED (install failed, offline?)\n' >&2
      return 0
    fi
  fi
  if [[ ! -f "$CACHE/eslint.config.mjs" ]]; then
    cat > "$CACHE/eslint.config.mjs" <<'CONFIG'
import tseslint from 'typescript-eslint';
export default tseslint.config({
  files: ['**/*.ts', '**/*.tsx'],
  languageOptions: { parser: tseslint.parser },
  rules: {
    complexity: ['warn', 10],
    'max-depth': ['warn', 4],
    'max-lines-per-function': ['warn', 120],
  },
});
CONFIG
  fi
  # eslint resolves the config's parser relative to the config file, so the
  # sources are linted where they are and only the config lives in the cache.
  local code=0
  (cd "$WEB" && "$CACHE/node_modules/.bin/eslint" --config "$CACHE/eslint.config.mjs" \
      -f json -o "$OUT/complexity.json" src) || code=$?
  node -e '
    const report = require(process.argv[1]);
    const hits = report
      .flatMap((file) => file.messages.map((m) => ({ file: file.filePath, ...m })))
      .sort((a, b) => score(b.message) - score(a.message));
    function score(message) { return Number((message.match(/of (\d+)\./) || [])[1] || 0); }
    const cyclomatic = hits.filter((h) => h.ruleId === "complexity");
    console.log(`functions over CC 10: ${cyclomatic.length}`);
    for (const hit of cyclomatic.slice(0, 10)) {
      console.log(`  ${hit.message}  (${hit.file.replace(process.cwd() + "/", "")}:${hit.line})`);
    }
    const long = hits.filter((h) => h.ruleId === "max-lines-per-function").length;
    const deep = hits.filter((h) => h.ruleId === "max-depth").length;
    console.log(`functions over 120 lines: ${long}, nesting over depth 4: ${deep}`);
  ' "$OUT/complexity.json"
  printf 'complexity: reported (advisory, never fails the build)\n'
  return 0
}

case "$layer" in
  dup) run_dup || status_total=1 ;;
  dead) run_dead || status_total=1 ;;
  complex) run_complex ;;
  all)
    run_dup || status_total=1
    run_dead || status_total=1
    run_complex
    ;;
  *) echo "usage: $0 [all|dup|dead|complex]" >&2; exit 2 ;;
esac

printf '\nreports: %s\n' "$OUT"
exit "$status_total"
