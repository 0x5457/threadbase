# 原始资料索引（Delta UI 调研）

抓取时间：2026-02，来源全部为公开页面。用途：作为 `docs/research/delta-ui.md` 的证据基座。

> `images/` 未提交（32 MB）。需要复现时按 `manifest-images.tsv` 第 3 列重新下载；本地调研期间的文件名与表中一致。

## 目录

| 目录 | 内容 | 数量 |
| --- | --- | --- |
| `docs/` | delta.dev/docs 下全部 `.md` 原文 | 58 |
| `blog/` | delta.dev/blog 下全部 `.md` 原文（含 The Diff 周报、planning、multiplayer 等） | 约 20 |
| `site/` | `llms.txt`、`docs-llms.txt`、首页与 /docs /download /cli /pricing /roadmap /deltadb /blog 的 HTML | 10 |
| `images/` | 官网与博客引用的截图，已下载 | 53 |
| `github/` | `zed-industries/delta-claude` 的 README（GitHub API 限流，未取到目录树） | 1 |
| `manifest.tsv` | 文档抓取记录：HTTP 状态、字节数、原始 URL、本地路径 | 58 行 |
| `manifest-images.tsv` | 图片抓取记录：状态、content-type、字节数、原始 URL、本地路径 | 53 行 |

文件名规则：URL 路径扁平化，`/docs/agents/comments` → `docs/docs__agents__comments.md`。原始 URL 以 `manifest*.tsv` 第 3 列为准。

## 已人工核对的截图（可直接引用）

| 文件 | 原始 URL | 内容 |
| --- | --- | --- |
| `images/17-nested_subthreads.webp` | images.zed.dev/blog/the-diff/2026-10/nested_subthreads.webp | 嵌套子 thread 的树：chevron 展开、缩进、消息气泡图标、子 thread 节点图标 |
| `images/21-web_mobile_thread_view_2.webp` | .../web_mobile_thread_view_2.webp | 移动端 thread 视图：标题 + `delta / main` 面包屑、`Thread / Changes +694 −85 / Files` 三个 tab、折叠的工具行、`Apply patch` 卡片、底部输入区（引用按钮、发送、模型与 effort 选择器） |
| `images/03-color_code_threads.webp` | .../color_code_threads.webp | 桌面端左栏：`Pinned` 与 `Projects` 分组、thread 左侧色条、嵌套行、右侧 diff；thread 右键菜单：Copy Share Link ⌘⇧S、Unpin ⌘⌥P、Rename F2、Regenerate Title、Mark as Unread ⌘⇧U、View Details、Copy Content、Copy as JSON、6 个颜色点、Archive ⌘W、Delete Locally |
| `images/16-multiple_terminals_2.webp` | .../multiple_terminals_2.webp | 终端面板：多个终端卡片（头像 + shell 名 + 下拉）、`Terminals` 菜单（New Terminal in cloud / delta / zed.dev / delta-claude）；提示 worktree 位于 `.delta/worktrees/...` |
| `images/37-initial-interaction-d.webp` | .../planning-in-delta/initial-interaction-d.webp | 桌面端主视图：左栏 `Projects` 下直接挂 thread（可展开 chevron）、thread 行带 agent 图标/成员头像/未读点；右侧单列时间线，多人头像、`Thinking ›` 折叠、标题层级、引用块（带作者头像）、正文内的锚点下划线 |
| `images/33-coles-subthread-d.webp` | .../planning-in-delta/coles-subthread-d.webp | 子 thread 相关（未逐张核对，交给调研确认） |

## 其他图片（按来源博客归类，内容待调研核对）

| 文件 | 来源 | 主题 |
| --- | --- | --- |
| `images/02-bookmarks.webp` | the-diff/2026-09 | 书签 |
| `images/05-create_organization.webp` | the-diff/2026-09 | 创建组织 |
| `images/06-custom_providers.webp` | the-diff/2026-09 | 自定义 provider |
| `images/07-git_worktrees.webp` | the-diff/2026-09 | git worktree 与 thread |
| `images/14-delta_configures_self.webp` | the-diff/2026-10 | Delta 自我配置 |
| `images/15-merged_colors_and_tags.webp` | the-diff/2026-10 | 颜色与标签 |
| `images/18-remote_mcp_servers.webp` | the-diff/2026-10 | 远程 MCP |
| `images/19-web_mobile_inbox_2.webp` | the-diff/2026-10 | 移动端 Inbox |
| `images/20-web_mobile_message_overview_2.webp` | the-diff/2026-10 | 移动端消息总览 |
| `images/31..38-*` | planning-in-delta | 桌面端多人规划、子 thread 的两组亮/暗截图 |
| `images/25..30-*` | escaping-the-tyranny-of-worktrees | 亮/暗两套 worktree 视图 |
| `images/39..46-*` | please-stop-sending-me-google-docs | 文档协作相关 |
| `images/11-ai_engineer_merch.webp`、`12-ai_engineer_team.webp` | the-diff/2026-10 | 营销图，非 UI |
| `images/22-24-*` | 官网团队头像 | 非 UI |

## 文档侧的重点文件

- `docs/docs__agents__threads.md`、`docs__agents__subagents.md`、`docs__agents__comments.md`、`docs__agents__review-and-sync.md`
- `docs/docs__collaboration__collaborate-thread.md`、`docs__collaboration__delta-on-the-web.md`
- `docs/docs__concepts__core-concepts.md`、`docs__concepts__worktrees.md`
- `docs/docs__configuration__keybindings.md`、`docs__configuration__settings.md`
- `blog/blog__planning-together-in-delta.md`、`blog__claude-code-multiplayer.md`、`blog__the-diff-2026-10.md`、`blog__escaping-the-tyranny-of-worktrees.md`

## 使用约定

1. 引用事实时附 `manifest.tsv` / `manifest-images.tsv` 中的原始 URL。
2. 只对「已人工核对」表中的截图写确定的布局结论；其余图片先 `read` 看过再写，没看过就标「未验证」。
3. 营销页与博客里的功能描述可证明「存在」，不能直接当交互规格；交互细节以文档与截图为准。
4. 无法确认的 UI 字符串、键位、面板宽度，一律进「未知项」。
