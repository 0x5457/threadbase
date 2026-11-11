# 1024/1280 before baseline (reconstructed)

The original capture in this directory (2026-11-11 01:15) was blank: every PNG had
`colors=1` (a single flat fill) because the screenshot run hit a blank page. The
1440/768 before images in `../before/` are original and valid.

To keep the 8-combination before/after requirement auditable, the 1024/1280
images here were re-captured from a reconstructed pre-change baseline:

- pre-change shell/theme/routes recovered from the Pi session reads of
  2026-11-10 16:50–17:03: `TitleBar`, `StatusBar`, `SidebarTree`, `WorkspacePage`,
  `ProjectPage`, `InboxPage`, `SettingsPage`, `ComparePage`, `InvitePage`,
  `NotFoundPage`, `theme.ts`, `index.css`;
- Thread/Composer visual values restored from the audit's pre-change
  measurements (32px composer radius, 148px min-height, 42px notice cards with
  background and trailing check, 11.5px author line, etc.);
- unchanged API/components come from the current tree.

They are **not** a build of the exact 16:48 working tree: that tree was
uncommitted and was overwritten by concurrent sessions. They are a visual
baseline for the old layout, not a claim that the original binary was recovered.
The `after/` images are the real current build.
