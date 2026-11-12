# Final verification — UI and interaction goal

Date: 2026-11-12. Dev API `127.0.0.1:7800` (fixture ACP), Vite
`127.0.0.1:5173`, Chrome 1440/820/390.

The first completion request was rejected. The follow-up below addresses every
auditor objection, and all screenshots/artifacts in this folder were produced
after the last source edit (`TitleBar.tsx`/`App.tsx`/`RealThreadView.tsx` 03:51,
`threads.rs` 03:59; matrix captured 03:59–04:01; behavior/evidence 04:02).

## Rejection follow-up

| Auditor objection | Resolution | Evidence |
|---|---|---|
| Sample identity leaked into Sidebar/Composer | `Sidebar` uses `GET /api/auth/me`; footer now shows the real `ada@example.com` / `@ada` / `A`. Composer avatar uses the real display name and draft user color. `Ada Lovelace` remains only in the Explicit `Preview → Design sample` avatars. | `screenshots-final/*/thread.png`, `behavior-smoke.json` checks 1–2 |
| `Default diff baseline` was inert | `threads::bundle` now loads user settings and passes `settings.default_baseline` into `diff::list`; the Changes panel consumes the bundle. UI saves invalidate `thread` queries. | `behavior-smoke.json` check 4; API probe below |
| Online indicators never lit | Title bar consumes `onlineUserIds` from the live `/ws` presence state through `RealThreadView`; bundle participants are also overlaid from `PresenceHub::online_user_ids`. | `behavior-smoke.json` check 3 (1 indicator), screenshot online dot |
| State residue across threads / reopen | Changes and Files panels are keyed by thread; `RealChangesPanel` clears/reselects and disables a stale diff; Review resets verdict/note/busy on open/thread change; Share resets username/email/link/busy/roles. | `behavior-smoke.json` checks 5, 6, 8 |
| Unreachable share-link branch | Removed the `share.shareToken` branch from `ShareDialog`; the share surface now only renders controls backed by the API (members, email/link invites, revoke). | `behavior-smoke.json` check 7; Share screenshots |
| Title-bar Inbox dot always on | `App` queries `/api/inbox?filter=unread`; `TitleBar` renders the dot only when `unreadCount > 0`. The matrix was captured with 2 real unread items (dot true); after capture they were marked read and `evidence/inbox-empty-*.png`/`inbox-empty.json` show unread 0/no dot. | `behavior-smoke` checks both branches |
| Cancelled subagent still offers Cancel | Cancel renders only for non-terminal subagents (`canCancel` and status not `cancelled`/`error`); the backend now also reports `canCancel: false` for `cancelled`/`error`, matching its existing rejection. | `behavior-smoke` check; final thread screenshot; API probe `Test investigator → cancelled, canCancel false` |
| Evidence matrix incomplete (Files/Share/Review/Invite; 820; 390 dark; real diff; populated Inbox) | 60 final screenshots: 10 surfaces × 3 widths × light/dark. Inbox is populated with 2 unread items; Files opens the real added `docs/presence.md`; Changes/Review use that real working-tree change. Empty/no-dot and Inbox→open-thread evidence supplement it. | `screenshots-final/`, `capture-matrix.json`, `evidence/` |
| Zero-console claim was prose-only | Reproducible Playwright smoke writes machine-readable results; the capture script also records console errors. Both are zero. | `console-smoke.json`, `capture-matrix.json`, `console-smoke.mjs`, `capture-matrix.mjs` |
| 7777 service is still the old release | The operator's standing instruction is **not** to restart the 7777 systemd unit; this goal verifies the rebuilt web app through the dev stack (5173 → 7800). Release install/restart is a separate deploy step, not silently performed. | constraint retained; documented here |

The API probe for the baseline (same thread, same working tree):

```
base_branch -> base_branch | main
last_turn   -> last_turn   | Last turn
last_commit -> last_commit | Last commit
```

The final state is `base_branch`; the pending invite used for the invite page was
revoked after capture (`pendingInvites: 0`).

The 60-image matrix was captured while two real Inbox items were unread, so the
top-bar dot is true in those frames. After capture they were marked read:
`evidence/inbox-empty-*.png` + `inbox-empty.json` show `unread 0`, no dot, and
the caught-up state. `evidence/inbox-open-thread.png` +
`inbox-open-thread.json` show clicking a populated Inbox row opens its thread.
`behavior-smoke` checks both the dot-on-unread and dot-cleared branches.

## Quality gates

| Command | Result |
|---|---|
| `pnpm --dir web typecheck` | PASS (exit 0) |
| `pnpm --dir web build` | PASS (`✓ built`; only the configured chunk-size warning) |
| `MBX_BLAZE=false cargo check -p threadbase` | PASS (exit 0) |
| `cargo fmt --all --check` | PASS (exit 0) |
| `pnpm --dir web smoke:console` | PASS: 12 steps, **0** console/page errors |
| `pnpm --dir web smoke:behavior` | PASS: 10/10 checks, **0** console errors |
| `pnpm --dir web capture:matrix` | PASS: 60 screenshots, **0** console errors |

`smoke:console` and `capture:matrix` need the dev API on 7800 and Vite on 5173;
`CHROME_PATH` defaults to `/usr/bin/google-chrome-stable`. See the scripts for
the exact steps and selector logic.

## Final screenshot matrix

`web/audit/screenshots-final/<width>-<theme>/`:

| Width | Themes | Surfaces |
|---|---|---|
| 1440 × 900 | light, dark | inbox (populated), thread, changes, files, review, share, settings, preview, preview-changes, invite |
| 820 × 1180 | light, dark | same 10 |
| 390 × 844 | light, dark | same 10 (drawers/modals full-width/full-screen) |

Notable final images:

- `1440-light/inbox.png` — populated Inbox (2 unread) with the real account, conditional top-bar dot, and Inbox row count.
- `1440-light/thread.png` — real account, live online dot, real `docs/presence.md` change, no sample identity; cancelled subagent has no Cancel action.
- `1440-light/files.png` — Pierre file viewer showing `docs/presence.md`.
- `1440-light/review.png` — review modal with the real change, approvals, comments, verdict.
- `1440-light/share.png` — real member and one link invite (revoked afterward).
- `1440-light/invite.png` / `390-dark/invite.png` — invite accept surface.
- `820-dark/*` and `390-dark/*` — tablet/phone dark coverage for every surface.
- `390-light/thread.png` — phone thread with the real composer avatar and working footer above the status bar.

Additional evidence folders: `screenshots-after/` (desktop visual-system pass),
`screenshots-responsive/` (drawer/diff wrapping), `evidence/` (interaction
states), and the ledgers `README.md`, `thread-interactions.md`,
`workspace-flows.md`, `responsive.md`. In `evidence/`,
`baseline-last-turn.png` shows the Changes header after switching the setting
(`Since Last turn`); `nav-footer-390-{light,dark}.png` show the corrected phone
sidebar height with the account/Settings row above the status bar;
`inbox-empty-*.png`/`inbox-empty.json` show unread 0 with no dot; and
`inbox-open-thread.png`/`inbox-open-thread.json` show a populated Inbox row
opening its thread.

## Audit closure

| # | Finding | Status |
|---|---|---|
| P1-1 | Sidebar hierarchy/density | CLOSED — 27px single-line rows, hover/touch project menu, guide line, status dots, no permanent action rows |
| P1-2 | Thread density | CLOSED — doc meta and repeated metadata removed, 9px rhythm, flat subagent/comment rows, stale stream caret cleared on turn end/idle |
| P1-3 | Double aside header / empty diff | CLOSED — one header, Changes/Files switch, first file auto-selected |
| P1-4 | Uppercase/pill status noise | CLOSED — dot + lowercase status, text kinds, no uppercase badge noise |
| P1-5 | Composer chrome | CLOSED — live dot + eye count, compact remote drafts, thread-comment action, dead Attach removed |
| P1-6 | Dead/unexplained controls | CLOSED — Inbox wired with a conditional unread dot, Bookmark/History/Attach/Expand removed, phone `…` menu is the full ThreadMenu, Inbox Mark all read, unreachable share-link branch removed, terminal subagents no longer show Cancel |
| P1-7 | Inbox unfinished | CLOSED — text filters, Mark all read, intentional empty state |
| P1-8 | Settings generic cards | CLOSED — flat hairline sections, dark switches, local theme source |
| P1-9 | Mobile/tablet chrome | CLOSED — full-width phone drawer, 520px tablet sheet, 30/32/36px targets, wrapped diffs, full-screen Share/Review; phone sidebar height now leaves the footer above the status bar |
| P1-10 | Data/state edge cases | CLOSED — review protocol text stripped; merge advances `base_sha`; `1 file` grammar; thread lists refresh; stale file/panel state reset |
| P2-11 | Settings persisted but not applied | CLOSED — baseline, text width, hidden presence, Enter-to-send, default agent/model, subthread limit, browser notifications |
| P2-12 | Presence only a count | CLOSED — live title-bar online indicators via `/ws`; bundle overlay; composer watching count. Per-character document carets remain P3. |
| P2-13 | Review not first-class | CLOSED as product decision — ReviewDialog is the first-class surface; the verdict is persisted as a thread comment (no separate review table). |
| P2-14 | Attachments only `@path` | P3 backlog — file references work; embedded attachments need an ACP/product decision |
| P2-15 | No terminal panel | P3 backlog — no terminal surface is exposed by the current model |
| P2-16 | Inbox mark-all/access | CLOSED — Mark all read; access/mention items open their thread |
| P2-17 | Fake sample island | CLOSED — only `Preview → Design sample`, clearly labelled |
| P2-18 | Sample identity in real views | CLOSED — real `me` in Sidebar/Composer; sample names only in Preview |

## Remaining P3 (explicit, not hidden)

1. Slash/@ autocomplete is a Mantine Paper list + keyboard handler; migrate to Tiptap Suggestion or Mantine Combobox for a single library-owned interaction.
2. File tree composes Mantine NavLink/ScrollArea; use a tree library if it grows.
3. A few inline styles remain in FilesPanel/composer remote blocks.
4. Per-character remote carets in the thread document (draft-block awareness exists).
5. Terminal panel and embedded file attachments (ACP capability/product decision).
6. Fixture ACP does not transition subagent status on cancel; the UI sends the request and surfaces errors.
7. A true second Tailscale identity and a real Codex/Claude ACP session were not run; the Yjs server path, presence socket, share/revoke, and invite page were exercised. The UI is adapter-agnostic.
8. The 7777 systemd service remains on the previous release by explicit operator instruction; this goal did not restart it. Dev verification is 5173 → 7800.

## Change surface

- Backend: `routes_files.rs` (recursive search), `routes_skills.rs` + `skills.rs`, `routes_workspace.rs` (merge base and presence overlay), `threads.rs` (settings baseline, capture overlay), `diff.rs` (baseline-aware stats for the status bar/list), `presence.rs` (online ids), `shared/api.rs` DTOs, `workspace/files.rs`.
- Frontend: Mantine 9 + Pierre + Tiptap + Yjs/y-websocket; Inbox, live thread, drafts, composer discovery, Changes/Files, Settings, Share/Invite, Review, ThreadMenu, responsive shell. Real account from `GET /api/auth/me`; live online IDs from `/ws`; dead share-token UI removed; state reset keys/effects; mobile sidebar height corrected.
- Evidence and reproducibility: `web/audit/{final.md,README.md,thread-interactions.md,workspace-flows.md,responsive.md}`, `screenshots-final/`, scripts `console-smoke.mjs` (`smoke:console`), `capture-matrix.mjs` (`capture:matrix`), `behavior-smoke.mjs` (`smoke:behavior`), and their JSON outputs.
