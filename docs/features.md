# Features in depth

The README lists what Threadbase does; this page explains how each part
behaves and which switches control it.

## The sidebar: status and placement are two different things

Each thread has two orthogonal states, each defined in exactly one place:

- **`thread` status** answers "what is the agent doing":
  `preparing / idle / running / requires_action / interrupted / error /
  archived`. `archived` lives here because it changes which commands the
  thread still accepts.
- **`placement`** answers "where did a person put it":
  `active / snoozed / settled`. Placement never affects the agent: a settled
  thread can still be prompted, and sending a message wakes it.

`Working` and **needs you** are not stored states; they are derived from
status, queued input, and live subagents, with one priority order
(`archived > pinned > snoozed > settled > working > active`). Within a
section, threads that need a human (unanswered approvals/questions, errors,
interruptions) come first. Sorting is computed once, server-side.

Placement commands: `POST /api/threads/{id}/placement` with
`settle / unsettle / snooze / wake` (and `until` for `snooze`; the server
defaults it to 24 h so two browsers cannot disagree about "tomorrow").
Illegal transitions are refused with a plain-language 409 ("wake this thread
before settling it"). Rules worth knowing:

- `settle` is refused while anything is running or waiting; it never cancels
  for you. It also clears the pin, so finished work cannot squat at the top.
- A prompt wakes a settled or snoozed thread; so does a new approval or
  question — a thread that asks for a human comes back into view.
- Project rows count their "needs you" threads, including folded projects.

**Auto-settle.** A sweep runs every 60 s. A thread settles itself when it has
been idle for `THREADBASE_SETTLE_DAYS` (default 7, `0` disables), has nothing
queued or running, and is not in an error/interrupted state — those need a
person, not a shelf. Silence is measured from the later of last activity and
the last placement change, so a just-woken thread is never re-shelved in the
same sweep. The thread menu's **Never auto-settle** opts one thread out.
Snoozed threads always wake when their deadline passes, whatever the settle
setting.

## The permission ladder

How much the agent may do without asking is one ladder, not an ad-hoc
decision:

| Mode | Behavior |
| --- | --- |
| `supervised` | Asks for every command and edit. Default. |
| `auto_edit` | Edits and reads pass automatically; commands and the rest still ask. |
| `full_access` | Nothing is asked. |

Resolution has one implementation: the thread's own choice → the project's
default → the instance default, then **clamped by the instance ceiling**.
Whoever runs the machine decides how much autonomy it hands out:

```bash
THREADBASE_PERMISSION_MODE=supervised        # instance default
THREADBASE_MAX_PERMISSION_MODE=full_access  # ceiling; auto_edit means no thread can go higher
```

- Options above the ceiling show as `· managed` and are disabled in the UI —
  the server would clamp them anyway.
- A user's standing rule ("always allow this") beats the mode: a rule is an
  explicit decision, a mode is a default posture. Every decision is recorded
  — rule matches name the user; mode passes name the system — so the
  transcript never claims a person approved something they never saw. Mode
  approvals still write an audit row.
- The effective mode is also pushed to the provider (`session/set_mode`) so
  long turns do not keep asking the server things it would pass anyway:
  `supervised`→Claude Code `default`, `full_access`→`bypassPermissions` /
  Codex `agent-full-access`, etc. Providers with no equivalent are simply
  asked per request; a provider that declares modes but not ours raises a
  visible warning instead of a silent downgrade. Thread changes apply to live
  sessions immediately; project defaults apply to the next session.

## Watching, notifications, and push

Activity makes you a watcher (creating, prompting, commenting, answering);
watch or mute explicitly from the thread menu — muting only silences
notifications. Approvals, questions, @mentions, finished turns and errors
notify the thread's unwatched-muted watchers, never the whole workspace, and
never the person currently looking at that thread (except errors). The inbox
pages through `GET /api/inbox`; unread counts arrive over each user's own
WebSocket. A thread with unread inbox rows also shows a dot in the sidebar;
opening the thread marks those rows read, like clicking them in the inbox.

The page is an installable PWA: the server generates and stores its own
VAPID keys on first use, and Web Push delivers the same notifications when the
tab is closed (iOS needs Add to Home Screen first).

## Agent sessions

Adapters are expected as commands on `PATH` (e.g.
`npm i -g @agentclientprotocol/codex-acp`); only when missing does the server
fall back to `npx`, which adds about a second per start — `threadbase doctor`
flags adapters still on npx.

- **Lazy start.** Creating a thread does not start a process; the first
  prompt does. The message that starts the adapter is not shown as queued: it
  is the thing being started, and the thread reads `preparing` until the turn
  begins. A failed start puts it back in the queue instead of losing it.
- **Idle shutdown.** Sessions close after `THREADBASE_IDLE_SESSION_SECS`
  (default 900, `0` = never); the binding remembers the ACP session id and
  resumes transparently, falling back to a context-reset notice if that
  fails.
- **Concurrency.** `THREADBASE_MAX_ACTIVE_TURNS` (default 512, `0` unlimited)
  caps turns running at once; sends past the cap become queued input, drained
  as permits free up.
- Deleting or archiving a thread stops its agent process first, so no agent
  keeps writing in a deleted worktree.

## Worktree setup command

A project's **Setup command** runs once in every new worktree (including
forks) before the agent starts — dependency installs, `.env` copies, service
boot. Progress shows as a banner; failures can be retried without recreating
the thread. Prompts are disabled while setup runs, queued input waits, output
streams into the event log (tail kept to 32 KB), and a server restart marks an
interrupted setup as failed rather than stuck. Timeout:
`THREADBASE_SETUP_TIMEOUT_SECS` (default 900).

## Search

`Cmd/Ctrl+K` opens search over thread titles, prompts, agent messages and
comments (SQLite FTS5), grouped by type. The index is maintained on write and
backfilled in the background on first start; `threadbase reindex` rebuilds
it. Queries are plain AND-matched words — no operators to escape.

## History, changes, and review

Every turn starts with a snapshot commit of the worktree, recorded as a
checkpoint. From there:

- The **History** panel lists commits since the thread's base, each mapped to
  the turn that produced it (by checkpoint order, not timestamps). Clicking
  one opens that commit's own diff; **This turn** widens it to the whole turn
  — from its start checkpoint to the next turn's start (or the working tree
  for the latest turn, so uncommitted work counts).
- **Line comments** anchor to path, hunk, line, side and a content hash; when
  the hunk shifts, the server re-locates by hash and, failing that, degrades
  to a file-level anchor marked `anchorChanged`. Anchors are only created on
  the base-branch baseline, where the math holds.
- One honest limitation: the turn span does not try to exclude files the
  agent brought in via its own `git pull`/`merge`. Distinguishing those from
  real edits needs rename/merge tracking that this model does not have, and a
  guess would make the view less trustworthy, not more.

## Editing a prompt, and rewinding

- **Edit** (hover a user message, or long-press on mobile) rewrites the
  message and drops every event after it: queued input, pending
  approvals/questions, related comment notifications, subagent rows. Other
  windows are told to reload. ACP sessions cannot be rewound, so the adapter
  binding starts fresh with the edited prompt; earlier turns do not re-enter
  its context, and files stay as they are — the dialog says both things.
- **Rewind** (the undo arrow on a user message) rolls conversation *and*
  files back: stop the adapter, `git reset --hard` to that turn's checkpoint +
  `git clean -fd`, then truncate the log — in that order, so a git failure
  leaves a consistent, restartable thread. Rewinding writes a system notice;
  without a checkpoint the server returns 409 instead of guessing.

## Forking

**Fork thread** (thread menu) copies the worktree, the agent binding and the
whole conversation into a new thread. **Fork from here** (on any of your user
messages) stops the transcript after that prompt's turn and starts the new
worktree at that turn's checkpoint, so the files and the conversation describe
the same moment. When the source has no checkpoint there — it runs directly in
the project, or git could not snapshot — the fork keeps the earlier transcript
and copies the source's current files, with a system notice saying so.

The fork gets its own ACP session, which cannot be branched. The copied
transcript therefore rides once with the first prompt as bounded context
(newest turns first, at most 48k characters); later turns are the adapter's
own. Checkpoints for the copied turns travel with it, so their per-turn diffs
still work; rewinding one of those turns is refused, because those files
belong to the thread the fork came from.

**Apply changes** (top menu) is deliberately local: it requires a clean
project tree on the thread's base branch, warns when the base has moved
(`baseDrift`), runs the project's optional **Verify command** in the worktree
first (5-minute timeout), and on commit failure resets back so a half-applied
state never survives. No branch switching, no pushing, no PRs.

## Attachments and drafts

The composer takes multiple files by picker, drag, or paste. Images up to
8 MB render inline with a lightbox; other files up to 25 MB (8 per message)
are stored under the data dir and served as forced downloads — never inlined
as HTML/SVG. Pasting over 8000 characters of text saves it as a `.txt`
attachment instead of flooding the editor. Agents receive images natively,
small text files inline, and large or binary files as paths with metadata.

Drafts are shared CRDT documents, so two people can write one prompt; a
thread with a draft shows a pencil mark in the sidebar and Sessions list.

## Optional features, off by default

Switches live in `~/.config/threadbase/env` (or the process environment).

### Browser terminal

```bash
THREADBASE_TERMINAL=1
```

A per-thread shell in a bottom drawer (`Ctrl+``), cwd following the agent
(worktree, project dir, or `$HOME` for a global shell). Sessions live in a
private tmux server — only in memory, never logged to disk or the event log —
so they survive refreshes and service restarts; close means detach.
**Members watch by default; only admins and names in
`THREADBASE_TERMINAL_DRIVERS` can type.** The WebSocket handshake uses a
one-time ticket bound to the session (30 s); the browser mints a fresh one on
every reconnect. Both ends probe the socket, so a tab that slept or lost the
network re-attaches on its own instead of painting a stale screen, and a socket
that went silent without a close frame is dropped after 90 s rather than
holding its viewer slot. `THREADBASE_TERMINAL_SOCKET` for
the private socket name, `THREADBASE_TERMINAL_SHELL` for the login shell.

### Host tmux sessions

```bash
THREADBASE_TERMINAL=1 THREADBASE_TERMINAL_HOST=1
# THREADBASE_TERMINAL_HOST_SOCKETS=default,work
```

List and attach to the machine's real tmux servers. Every member can see and
take over (read-only accounts get nothing); one browser drives at a time, with
a visible **Take over** handoff; the previous driver's input is dropped
server-side. The workbench's private socket is excluded, so this cannot
bypass the per-thread read-only policy. This switch is equivalent to "give
every member an interactive shell as the service account" — only for teams
that trust each other. Contents are never logged.

### Sessions view

The Sessions page (sidebar or command palette) is one list of "what can be
opened now": threads, their live workbench shells, and host tmux sessions,
sorted pinned → running → recent. A tmux session whose cwd falls inside a
registered project offers **+** to spawn a thread bound to it; the link
records a `(socket, session id)` fingerprint so a recycled session id is never
misattributed. Deep links: `/sessions?session=tmux:<socket>:<id>`.

## Event log and compaction

Each thread is an append-only event log (per-thread order, cross-thread
parallelism), projected into thread state and broadcast over WebSocket.
Streaming writes a row per chunk, so `threadbase compact` exists: it rewrites
the log with the same folds a cold load uses (finished messages keep only
their completed row; plans and usage are preserved), leaving replayed
transcripts identical and old databases much smaller. Run it online; see
[deployment.md](deployment.md#maintenance) for `--vacuum`.
