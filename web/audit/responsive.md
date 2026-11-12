# Responsive verification — 820 / 390, light + dark

Screenshots: `screenshots-responsive/{820,390}-{light,dark}/` (Inbox, thread,
changes drawer, files drawer, settings). Additional evidence in `evidence/`:
`resp-820-thread2.png`, `resp-390-changes2.png`, `mobile-review2.png`,
`mobile-share2.png`, `resp-390-preview-changes.png`.

## Results

| Check | 820 | 390 |
|---|---|---|
| Document width vs viewport | `820/820` on Inbox, thread, changes, settings | `390/390` on Inbox, thread, changes, settings |
| Sidebar | stays as 232px rail; thread column fits | full-height nav drawer; project actions visible on touch |
| Right panel | 520px right drawer; close in panel header; no underlying controls blocked | full-width drawer; panel header has Changes/Files switch + close |
| Diff wrapping | `DiffView` wraps at ≤75em; file diff no horizontal page overflow | same; demo diff wrapped at 390 (`resp-390-preview-changes.png`) |
| Composer | 28px tool targets; send and model controls visible | 32px tool targets; comment/@/command, eye count, model, send visible |
| Header | 30px icon targets; real actions remain reachable | 36px targets; Reviews/Share/Merge/rename/pin/fork/revert/archive in the `…` menu |
| Modals | — | Share and Review are full-screen (390px wide), scroll correctly, close reachable |
| Comments | per-message affordance is hover/desktop; thread comment remains in composer | no icon column; thread comment available; comment actions always visible |
| Focus | drawer opens focused on Close (not a random action) | same |

## Fixes made for this pass

- `DiffView` wraps through tablet widths (`74.99em`), not only phones.
- 820px header/action targets 30px; sidebar rows 32px; composer tool buttons 28px;
  file rows 32px. Phone keeps 36/32px targets.
- Drawer at 390 is full width; tablet stays a 520px right sheet.
- Comment affordance no longer paints an icon column on mobile/tablet; desktop
  reveals it on hover/focus.
- Mobile `…` menu is the full thread menu; Share/Merge/Reviews/revert/fork/archive
  all have handlers (previously the mobile menu's Share/Merge/Reviews were inert).
- Drawer close receives initial focus; focus ring is 1px/offset 1px instead of a
  heavy 2px ring.
