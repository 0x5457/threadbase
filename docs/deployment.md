# Deployment

Threadbase runs as one process on one machine. The web client, SQLite, and all
state live on that machine; teammates reach it through Tailscale. This guide
covers identity, installation as a service, upgrades, and the maintenance
commands.

## The model: loopback in, Tailscale identity

The server binds to a loopback address only. The single supported entry point
is `tailscale serve`, which terminates tailnet HTTPS and proxies to
`127.0.0.1:7777`. No public ingress, no tunnel provider, no OAuth app, no
passwords.

Sign-in is one POST (`/api/auth/tailscale`). The proxy presents
`Tailscale-User-Login` / `Tailscale-User-Name` /
`Tailscale-User-Profile-Pic`, and the server trades that identity for a
server-issued session cookie (`HttpOnly`, `SameSite=Lax`, plus `Secure` when
the public origin is HTTPS).

Trust rules, in one place:

- Those headers count **only when the socket peer is loopback** — that is,
  only the local `tailscaled` can present them. Any process that can already
  reach the loopback port can read the database anyway, so the boundary is
  honest rather than "trust any header".
- If the proxy forwards the client address (`X-Forwarded-For`), the claimed
  identity is checked against `tailscale whois` for that address. Mismatches
  are refused with 403.
- The first identity to sign in becomes the administrator. Pin it instead with
  `THREADBASE_ADMINS=you@example.com,teammate@example.com` (compared
  case-insensitively); later sign-ups land as `pending` until an admin approves
  them in the users page.
- Revoking an account (un-approve, or set disabled) invalidates its sessions
  immediately. A full reset is deleting `threadbase.db`.

CSRF: every non-GET request must carry `X-Requested-With: threadbase` and an
`Origin` naming this server (or `THREADBASE_PUBLIC_ORIGIN`). `tailscale serve`
preserves the browser's Host, so no extra configuration is needed.

## Install as a service

```bash
threadbase install --admin you@example.com
# then open https://<machine>.<tailnet>.ts.net:8443
```

One command configures the **current binary** as a user service — systemd on
Linux, launchd on macOS — and runs
`tailscale serve --bg --https=<https-port> http://127.0.0.1:<port>`. Nothing
is built, copied, or downloaded: the unit's `ExecStart` is this executable.

What it writes:

- `~/.config/systemd/user/threadbase.service` or
  `~/Library/LaunchAgents/threadbase.plist`, with the bind address, data
  directory, `THREADBASE_PUBLIC_ORIGIN` (from `--public-origin`, else derived
  from `tailscale status --json`), `THREADBASE_ADMINS`, and a deduplicated
  `PATH` (agent CLIs often live in mise/nvm/Homebrew paths).
- Secrets and optional switches belong in `~/.config/threadbase/env`
  (`THREADBASE_ENV_FILE`). **The service file only names that path**; the
  binary loads the file itself, so manual runs and `threadbase doctor` read
  the same settings.
- Linux: `loginctl enable-linger` + `systemctl --user enable/start`.
- macOS: `launchctl` load/start, with `AbandonProcessGroup` (tmux survives
  restarts), `KeepAlive{SuccessfulExit:false}`, throttling, and
  `ProcessType=Interactive`.

Options: `--print` (show, change nothing), `--port` / `--https-port`,
`--unit` (run several instances side by side), `--no-serve` (own proxy),
`--no-start`, `--keep-awake` (macOS power assertion), `--uninstall`.

### Why not the stock service-manager templates

The `service-manager` crate's systemd template omits `KillMode`, so a restart
would also kill the tmux server and every browser terminal; its launchd
template omits `AbandonProcessGroup` and log paths, and does not support an
environment file. So lifecycle goes through the crate, but the unit contents
are rendered here (systemd text, `plist` crate), and upgrading the dependency
only has to keep the `contents` contract.

## Upgrades

An upgrade is a new binary plus a restart:

```bash
threadbase upgrade              # download + verify the latest release, self-replace
threadbase service restart      # then bounce the service
```

Releases come from GitHub Releases (`threadbase upgrade --check` to peek).
Fresh installs: `curl -fsSL https://raw.githubusercontent.com/0x5457/threadbase/main/scripts/install.sh | sh`.
Linux binaries are portable; the darwin binaries are unsigned, so a
browser-transferred copy needs `xattr -d com.apple.quarantine`.

### Staged deploys (wait for an idle moment)

Restarts interrupt running turns, so the server can pick its own moment:

```bash
# on the server
THREADBASE_AUTO_DEPLOY=1

# from a dev machine: build, deliver, do not install
scripts/deploy.sh --stage
```

The server polls `<data dir>/incoming/` every 15 s. A staged binary is
installed only when nothing is running or waiting: no
`preparing/running/requires_action` threads, no pending approvals or
questions, no queued input, no live subagents. A pending approval blocks the
swap by design. It backs up the current binary, replaces it, consumes the
staged file, stops the agent adapters and exits with code 75. The service
manager starts the new binary; the server never restarts itself, because a
restart issued from the process being stopped cannot complete (it waits for
that process to exit). `Restart=always` on systemd and
`KeepAlive.SuccessfulExit=false` on launchd both read that exit as "start me
again". If it stays busy, it just waits and logs. `GET /api/health` exposes
`deployStaged` so you can see it arrived.

Prefer manual control? `scripts/deploy.sh --wait-idle` builds first, then
polls the database until the same idle predicate holds (gives up after two
hours, never kills anything). `scripts/deploy.sh --rollback` restores the
last backup.

## macOS specifics

- **The service needs a logged-in session.** LaunchAgents run after login;
  a always-on host should enable automatic login (blocked by FileVault, which
  `threadbase install` detects and reports).
- **Sleep:** `--keep-awake` holds an IOKit power assertion (like
  `caffeinate -s`); it dies with the process. Laptops that sleep on lid close
  are poor hosts.
- **Tailscale CLI:** found on `PATH`, else at
  `/Applications/Tailscale.app/Contents/MacOS/Tailscale`, else
  `THREADBASE_TAILSCALE`. It is used for both ingress (`serve`) and identity
  (`whois`), so it must be findable.
- **Logs:** `~/Library/Logs/threadbase/threadbase.log`, rotated daily with 7
  kept, plus `launchd.*.log` for early output. Linux uses journald;
  `THREADBASE_LOG_FILE` switches to a file anywhere.

## Maintenance

- **Event log size.** Every streamed token chunk is a row; long histories
  grow. `threadbase compact` folds superseded rows with the same rules a cold
  page load uses (replayed transcripts are unchanged). Safe online, in one WAL
  transaction. `--dry-run` reports; `--vacuum` reclaims disk and wants the
  service stopped.
- **Search index.** `threadbase reindex` rebuilds the FTS5 index; normally
  it is maintained incrementally and backfilled on first start.
- **Ops variables:** `THREADBASE_LOG_JSON=1` (structured logs),
  `THREADBASE_SESSION_DAYS` (session lifetime, default 30),
  `THREADBASE_WEBHOOK_URL` (per-thread notifications; the thread creator's
  saved webhook takes precedence).
