# Development

How the codebase is organized, and the gates it has to pass.

## Dev loop

```bash
pnpm --dir web install --frozen-lockfile
THREADBASE_DEV_IDENTITY=dev cargo run -p threadbase   # http://127.0.0.1:7777
pnpm --dir web dev                                    # :5173, proxies /api and /ws to :7777
```

The dev identity is honored only for loopback peers with no proxy headers,
so requests arriving through `tailscale serve` can never hit it. `mise run
start` does the one-command build-and-run.

For browser automation against a scripted agent, `scripts/e2e-server.sh`
builds the fixture adapter and runs the server on :7790 with a scratch data
directory.

## Gates

`scripts/verify.sh` runs everything in parallel and prints one PASS/FAIL per
gate:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
pnpm --dir web typecheck
pnpm --dir web doctor:ci        # React Doctor, correctness errors only
pnpm --dir web build
```

A real-adapter smoke test exists for Codex (needs `codex-acp` and a signed-in
CLI):

```bash
cargo test -p threadbase --test real_codex -- --ignored --nocapture
```

## Layout

```
crates/shared    wire + domain types; ts-rs generates web/src/api/types
crates/core      pure policy: state machines, delivery, permissions
crates/workspace paths, git worktrees, file browsing
crates/draft      Yjs draft documents
crates/agent      ACP harness, adapter profiles
crates/server     HTTP/WS service, SQLite event log, routes
web/              Vite + React 19 + Mantine client, embedded into the binary
```

Server modules by responsibility: `app` (assembly), `projection` (read model
+ WS broadcast), `threads` (lifecycle, worktrees, DTOs), `agents/`
(`AgentManager`, event pump, queue worker), one module per entity
(`approvals`, `elicitations`, `queue`, `comments`, `subagents`, `inbox`,
`watchers`, …), `events` (append-only log), `diff` (baselines and review),
`routes_*` (auth, parsing, delegation). Frontend: `web/src/api` is the fetch
layer, `components/thread/*` the timeline, `components/composer/*` the
composer; chunks are split per surface, and event-stream updates batch at
20 fps so long streams never block interaction.

## State machines

Every entity's legal states, transitions, and commands live in
`crates/core/src/state/` — one file per machine, no status-string
comparisons anywhere else. `threadbase machines` prints the catalogue;
`GET /api/machines` and `web/src/api/state-machines.json` are the same copy
(a test writes the JSON, and fails until you commit it). `ThreadDto
.allowedActions` and the frontend's `can()` / `threadCan()` both derive from
it, so a button and the server can never disagree. Machines distinguish
commands (`prompt`, `cancel`) from adapter facts (`turn_ended`): facts never
appear as buttons.

Adding one: create `XxxMachine` with a `transition`, implement `States` and
`Actions` for it, register in `state/mod.rs` and `catalogue()`, and add the id
to `web/src/lib/machine.ts`. The type the frontend reads is generated, not
hand-written.

## Code health

Beyond the gates, three diagnostics (not blocking, reports in `.quality/`):

```bash
scripts/quality/measure.sh dup      # jscpd: duplication, threshold 1%
scripts/quality/measure.sh dead     # knip: unused files/exports/deps
scripts/quality/measure.sh complex  # eslint: complexity >10, functions >120 lines
```

The rules for acting on the results are in `scripts/quality/review.md`:
generated files are skipped, symmetric API wrappers usually mean a product
gap rather than dead code, and deletions must still pass typecheck, doctor,
and build.

## Release

```bash
cargo build --release -p threadbase
```

GitHub Actions builds per-target binaries and checksums
(`.github/workflows/release.yml`); `scripts/install.sh` is the curl-able
installer; `threadbase upgrade` self-replaces the running binary (see
[deployment.md](deployment.md) for staged restarts).
