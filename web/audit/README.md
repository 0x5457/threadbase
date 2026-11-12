# Threadbase UI / interaction audit — 2026-11-12

> Final status and rejection follow-up: see [final.md](./final.md).

Scope: the current build served by `pnpm --dir web dev` (5173) against the local dev API
(7800). Light and dark, 1440 / 820 / 390. Screenshots in `screenshots/{light,dark}/`.

Method: every reachable surface was opened by driving the browser through the real
Inbox → project → thread path, with the fixture ACP agent where an agent was needed.
The intent was to inventory the product and rank reproducible defects, not to change
product code. (The sidebar density patch landed immediately before this audit and is
included in the screenshots.)

## Evidence index

| Surface | 1440 light | 1440 dark | 820 light | 390 light |
|---|---|---|---|---|
| Inbox | `1440-inbox.png` | `1440-inbox.png` | `820-inbox.png` | `390-inbox.png` |
| Thread | `1440-thread.png` | `1440-thread.png` | `820-thread.png` | `390-thread.png` |
| Changes | `1440-changes.png` | `1440-changes.png` | `820-changes-drawer.png` | `390-changes-drawer.png` |
| Files | `1440-files.png` | `1440-files.png` | `820-files-drawer.png` | `390-files-drawer.png` |
| Settings | `1440-settings.png` | `1440-settings.png` | `820-settings.png` | `390-settings.png` |
| Review | `1440-review.png` | `1440-review.png` | — | — |
| Project settings | `1440-project-settings.png` | `1440-project-settings.png` | — | — |
| Spotlight | `1440-spotlight.png` | `1440-spotlight.png` | — | — |

## Surface inventory

1. **Inbox** — Unread/All, row, open thread, empty state.
2. **Thread** — live event stream, user/agent messages, thinking row, tool call rows,
   diffs, comments, approval card, elicitation form, queue bar, subagents, composer.
3. **Composer** — plain text, slash commands, skills, @file search, model/effort menu,
   delivery mode, send, live/draft state, remote pending drafts.
4. **Changes** — since-base selector, stats, file list, file diff, close/expand, Files switch.
5. **Files** — breadcrumbs, folder navigation, file read, Pierre file view, close.
6. **Settings** — appearance, defaults, presence/notifications, save.
7. **Review** — changes summary, approvals, comments, verdict, submit.
9. **Project settings** — name, directory, branch, remote, save.
10. **Project / thread creation** — add project directory browser, new thread form.
11. **Spotlight** — Inbox, Settings, real threads, theme.
13. **Shell** — title bar actions, theme toggle, sidebar tree/project menu, status bar,
   mobile nav drawer, changes drawer.

## Findings

### P1 — blocks the “Delta-like, finished product” bar

1. **Sidebar hierarchy is improved but not finished.**
   Evidence: 1440 light/dark. Rows are single-line now, but project actions are
   hover-only (not discoverable on touch), the tree has no vertical guide lines, and
   Pinned/Preview sections have no empty-state or overflow policy.
   Repro: open 1440 with a project, then touch/pen the project row.
   Fix direction: keep 27px rows; show project actions on touch; add hairline guide
   lines; define Pinned/Preview empty states.

2. **Thread document density is far from Delta.**
   Evidence: 1440-thread. Repeating `name + timestamp` above every message, 14px block
   gaps, wide comment cards, large subagent cards, and a document meta row duplicate
   the status bar. Delta reads as one continuous document.
   Repro: open `Wire the real thread v2` and scroll.
   Fix: remove the doc meta row; hide repeated agent metadata; tighten block rhythm;
   convert subagents/comments to flat hairline rows; keep the composer as the only
   persistent footer.

3. **Right panel has two headers and is empty until a file is clicked.**
   Evidence: 1440-changes/dark. Top `Changes | Files` tabs plus a second `Changes`
   header; diff area empty although the thread has a changed file.
   Repro: open a thread with a change, look at the aside.
   Fix: one header (tabs/stats/file switch in one row); auto-select the first changed
   file so the diff is visible immediately.

4. **Uppercase/pill status noise.**
   Evidence: `RUNNING`, `REQUIRES ACTION`, `COMPLETED`, `PENDING`, `APPROVAL`,
   `QUESTION` badges in thread, subagents, comments, inbox.
   Repro: any thread; inbox.
   Fix: `Badge { text-transform: none }` globally, then replace status badges with a
   dot + lowercase text; remove comment `PENDING`, inbox kind pills.

5. **Composer chrome.**
   Evidence: 1440-thread bottom. A `live` row plus remote draft boxes sit above the
   editor; the model/effort label is tiny at the far right. Delta’s composer is a
   single calm input with small controls.
   Repro: open a thread, focus the composer.
   Fix: move the live dot next to the model label; keep remote drafts as one compact
   line; align controls on one 26px row.

6. **Dead or unexplained controls.**
   - Title bar Bookmark, History, Inbox icons: no-op.
   - Composer Attach: no-op.
   - Pinned section `+`: no-op.
   - Panel “Expand” icons: no-op.
   - Settings browser notifications/webhook are saved but have no runtime effect.
   Repro: click each.
   Fix: wire, remove, or mark as disabled with a reason; no silent no-ops in a finished app.

7. **Inbox is visually unfinished.**
   Evidence: 1440-inbox. Heavy segmented pill, uppercase `APPROVAL` badge, a single row
   floating in a 900px column with no secondary column or context.
   Repro: open Inbox with one item.
   Fix: small text tabs, kind as muted text, tighter row, optional right-side
   “recent threads” context or a centered empty state that reads intentionally.

8. **Settings is generic Mantine.**
   Evidence: 1440-settings. Three rounded cards, red-branded switches (now dark in code
   but visually inconsistent), default inputs; no relationship to the dense Delta surface.
   Repro: open Settings.
   Fix: one hairline section list; label/value rows; 28px controls; no card stack.

9. **Mobile/tablet chrome still competes.**
   Evidence: 390-changes-drawer/files-drawer/settings, 820. Drawer leaves a 30px
   sliver of the page, the settings page has the same wide cards, composer live/draft
   rows crowd the small viewport.
   Repro: 390 width.
   Fix: full-width sheets or a deliberate 8px overlay; one-row composer; mobile-first
   settings rows.

10. **Data/state edge cases surface in the UI.**
    - A pending review comment is prepended to the next prompt as raw
      `Review comments to address: … (anchor: …)`; users see protocol text.
    - Merge leaves `changes` showing the already-merged file (baseSha not advanced).
    - `1 files` grammar.
    - A running thread can show stale sidebar status until the 3s refetch.
    Repro: approve/comment/merge then inspect.

### P2 — polish and consistency

11. `Settings` text width / default agent / default model / subthreads are persisted
    but not applied to the workspace.
12. Presence is only a `N watching` badge; no live participant indicators in the title
    bar and no focus/cursor rendering in the document.
13. `Review` verdict is stored as a thread comment, not a first-class review record.
14. File attachment is a text `@path` only; no embedded file content / attachment rail.
15. No terminal panel; tool `terminalId` is not rendered.
16. Inbox has no mark-all-read or access-request actions.
### P3 — library / implementation hygiene

19. Slash/@ autocomplete is a custom list + keyboard handler over Mantine Paper; it
    should be migrated to Tiptap Suggestion or Mantine Combobox if we want a single
    library-owned interaction. It works today.
20. The file tree is composed from Mantine `NavLink` + `ScrollArea`; no tree library.
    Acceptable, but a full file tree should use `react-arborist`/`@headless-tree` if it
    grows.
21. Some one-off inline styles in FilesPanel/composer remote blocks; centralize in the
    design tokens/CSS when touching those files.

## Library compliance

- Components: Mantine 9 (buttons, menus, dialogs, tooltips, tabs, nav links, cards,
  scroll areas, notifications, Spotlight).
- Diffs/file rendering: `@pierre/diffs` (read-only file and diff views).
- Composer: Tiptap; drafts: Yjs + y-websocket.
- Data: TanStack Query; icons: Tabler; CSS: Mantine styles + one app stylesheet.
- Remaining custom interaction code: slash/@ suggestion menu, thread document layout.
  These are composition, not a replacement component library. The suggestion menu is
  the one item to migrate if “no custom interaction primitives” is strict.

## Audit conclusion

The product is functionally broad but visually unfinished: the shell is still a set of
Mantine boxes, not one Delta-like surface. The highest-leverage fixes are density,
status language, the right-panel header, the thread document rhythm, and the composer.
The functional gaps that block “finished” are the no-op controls, settings that do not
apply, live presence/cursors, and first-class review/attachments/terminal surfaces.
