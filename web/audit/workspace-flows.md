# Workspace, Inbox, Settings, Share, Search — verification

Evidence: `screenshots-after/1440-{light,dark}/inbox.png`, `.../settings.png`,
`evidence/new-thread-defaults.png`, `evidence/share-member-added.png`, and the
live dev API at 7800.

## Results

| Flow | Result | Evidence |
|---|---|---|
| Inbox unread → read → open thread | PASS | Clicking an item loads the thread, `POST /inbox/read` updates the unread count; the sidebar and Inbox count agree. |
| Inbox Mark all read | PASS | Unread 1 → no count, 0 rows, sidebar count clears; `POST /inbox/read {all:true}`. |
| Add project (server directory picker) | PASS | `/api/fs/directories` browser, git-only guard, `POST /api/projects`; verified earlier with `/tmp/tb-picker-repo`. |
| Project settings | PASS | Rename `tb-picker-repo → tb-picker-lab → tb-picker-repo` via the modal and `PATCH /api/projects/{id}`; path and branch preserved; sidebar updated. |
| New thread | PASS | Modal creates a real thread, selects it, invalidates the project thread query; settings defaults applied. |
| Settings save | PASS | `PATCH /api/settings`; hidden presence off/on, transcript width full, send-with-modifier, default agent/model. Theme now saved from the local color scheme so server/local cannot diverge. |
| Settings apply | PASS | Full transcript width produced a 900px doc at 1920; hidden presence removed the watching count; Enter sends when modifier requirement is off; New thread modal inherited Codex + gpt-5; browser notifications request permission when enabled. |
| Share: invite / accept / revoke | PASS | Email invite returns a token link; `/invite/{token}` accepts and redirects to Inbox; revoke removes the pending row. |
| Share: members / roles | PASS | Add `max` → Editor; role change now uses `PATCH /members/{id}` and becomes Viewer; remove; last-owner demotion returns 409 and is surfaced. |
| Spotlight | PASS | Real thread/project actions; search `Wire` opens the real thread; Inbox/Settings/Preview/theme actions work. |
| Refresh persistence | PASS | Pinned thread, project name, share members, settings and theme survive reload; local theme is the source of truth for the saved theme. |
| Error hygiene | PASS | No unexpected 4xx/5xx in the UI flows; the only 409 observed was the deliberate last-owner protection test. |

## Notes

- `SettingsView` now saves the local color scheme as `settings.theme`; the backend
  legacy default (`dark`) no longer overrides the UI color scheme on save.
- The `defaultModel` / `webhookUrl` fields can be cleared by sending an empty string;
  the backend's nullable patch cannot clear a value with `null`.
- Browser notification permission is requested only when the switch is enabled; actual
  notification display is gated on `Notification.permission === "granted"`.
