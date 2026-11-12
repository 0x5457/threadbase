# Thread interactions — fix and verification pass

Goal task: thread-interactions. Dev API 7800 + fixture ACP; browser at 5173.
Screenshots in `evidence/`.

## Results

| Interaction | Result | Evidence / notes |
|---|---|---|
| Live event stream (WS) | PASS | `/ws` subscribe, events_page/resync, live events; polling removed from RealThreadView. Subagent events filtered from the main document. |
| Live presence | PASS | Composer eye + count from `presence.focuses`; hidden-presence setting now sent by the hook and removes the count. |
| Multiplayer drafts | PASS | Yjs `blocks` array over `/ws/drafts/{id}`; author's block mirrored from Tiptap; remote pending block rendered read-only. Synthetic second author block displayed; `fromDraft` send clears through the server. See `max-draft-ui.png`. |
| Plain Enter vs modifier | PASS | Settings `sendWithModifier=false` makes Enter send; modifier true keeps newline. Verified with `plain-enter-send.png`; setting restored to true afterwards. |
| Slash commands / skills | PASS | `/comp` → `/compact `; `/diag` → `/diagnose-crash` with source; per-agent catalogs. |
| @ file references | PASS | server `files/search`; `@READ` → `@README.md `. |
| Queue | PASS | slow prompt + queued delivery; queue bar + drop; verified earlier in the same dev thread. |
| Approvals | PASS | pending tool suppressed, one approval card, Allow once resolves and status returns to idle. |
| Elicitation | PASS | schema-driven Select; Accept returns `elicitation:accept` + content. |
| Comments create/resolve/send/delete | PASS | create under selected message; Resolve shows `· resolved`; Send now sets status delivered; Delete removes the row (API checked). |
| Subagents display/cancel | PASS / fixture limit | flat rows with dot status; cancel calls the real endpoint. The fixture does not transition its synthetic subagents, so the row stayed `requires action`; this is an adapter limitation, not the UI. |
| Changes + Files | PASS | single panel header, first-file auto-selected diff, Files switch, Pierre file view, wrapped diff in the drawer. |
| Merge | PASS | confirmation modal → `/merge-back`; toast `Merged 8d8d215`; base repo received the change. |
| Revert | PASS | confirmation → `/revert`; changes changed from `+1 −0 / 1 file` to `+0 −0 / 0 files`. |
| Fork | PASS | thread menu → new `(fork)` thread selected. |
| Archive | PASS | thread menu → Inbox; thread removed from the project list. |
| Rename / pin | PASS | title updated everywhere; pinned thread appeared in Pinned. |
| Share / review | PASS (share role API fixed) | invite create/revoke and invite accept work; role changes now use `PATCH /members/{id}` (last-owner protection) instead of the upsert `addMember`; Review writes a thread comment and closes. |
| Duplicate approval/tool row | FIXED | a pending tool is hidden while a pending approval exists. |
| Review-comment protocol text in user prompt | FIXED | `Review comments to address:` suffix stripped from the displayed user message. |
| State residue on thread switch | FIXED | RealThreadView is keyed by thread id; comment editor, queue, pending cards and aside tab reset. |

## Dead controls cleared

- Removed Bookmark/History title-bar icons (no backing surface).
- Wired the title-bar Inbox icon to Inbox.
- Removed the demo panel Expand icon (no action).
- Removed composer Attach (no action; `@` is the file path).
- Kept project actions in a real Mantine `Menu` (New thread / Project settings), shown on hover and on touch.

## Limitations

- The fixture adapter does not implement subagent cancellation state changes; the Cancel request is sent and errors are surfaced, but the synthetic status does not change.
- A true second Tailscale identity was not run in this pass; the remote draft was verified with a second Yjs writer (same authenticated socket) and the presence count with one main socket. The CRDT path is the real server path.
- Real Codex/Claude ACP sessions were not exercised in this pass; all UI behavior is adapter-agnostic and the backend fixture covers the protocol events.
