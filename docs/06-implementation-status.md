# 实现状态与决策记录

配套文档：[01-requirements.md](01-requirements.md)、[02-open-decisions.md](02-open-decisions.md)、[03-tech-stack.md](03-tech-stack.md)、[04-subagents-and-questions.md](04-subagents-and-questions.md)、[05-ui-spec.md](05-ui-spec.md)、[07-state-machines.md](07-state-machines.md)、[08-state-model-decision.md](08-state-model-decision.md)。

本文件记录实现阶段的事实：实际版本、与规格的偏差、需求覆盖和验证证据。规格冲突在这里给出裁决，不再散落到代码里。

---

## 1. 版本审计（2026-11-10）

文档采样的版本部分已过时。下表是实现实际采用并通过构建的版本；未列出的依赖按 `Cargo.lock` / `pnpm-lock.yaml` 为准。

| 依赖 | 文档版本 | 实际版本 | 原因 |
| --- | --- | --- | --- |
| `agent-client-protocol` | 5.0 | **5.1.0** | 当前稳定版；5.x 同一 semver 线，启用 `process`、`unstable_subagents`、`unstable_end_turn_token_usage` |
| `sqlx` | 0.9 | **0.8.6** | `tower-sessions-sqlx-store 0.15.1` 固定依赖 sqlx 0.8；最终会话改为自建表，保留 0.8 以避免双驱动 |
| `yrs` | 0.28 | **0.17.4** | `y-sync 0.4.0` 固定依赖 yrs 0.17；y-sync 是当前唯一可直接对接 Yjs 13 的服务端同步实现 |
| `y-sync` | 0.4 | **0.4.0** | 与上一条配套 |
| `notify` | 8.2 | **8.2** | 9.0 仍为 RC，不用 |
| `ulid` | 未指定 | **1.2** | 2/3 有 API 变化，1.x 足够 |
| `ts-rs` | 12 | **12.0.1** | 启用 `no-serde-warnings` 与 `serde-json-impl` |
| `Vite` | 8 | **8.3.7** | 当前稳定 |
| `React` | 19 | **19.3.1** | 当前稳定 |
| `TypeScript` | strict | **5.9.3** | TS 7 是新的原生工具链，生态兼容性未验证；5.9 为稳定选择 |
| `React Router` | 8 | **7.18.5** | 7 是当前稳定大版本；data router 能力满足深链接需求 |
| `Mantine` | 9 | **9.8.0** | `@mantine/core` + `@mantine/hooks` + `@mantine/notifications` + `@mantine/spotlight`；React 19 兼容 |
| `shiki` | 4 | **4.6.0** | 当前稳定 |
| `y-websocket` | 未指定 | **3.1.0** | 客户端 Yjs sync/awareness 传输 |
| `Playwright` | 未指定 | **1.65.0** | 浏览器 e2e；使用本机 Chrome/Chromium |
| `pnpm` | 未指定 | **10.34.9** | 通过 `packageManager` 固定 |
| `cargo-nextest` | 未指定 | **0.9.151** | 测试运行器 |
| `tower-sessions` | 0.15 | **未使用** | 会话写入自建 `auth_sessions` 表，才能按用户批量撤销并保留审计；tower-sessions 的 store 表不提供该语义 |

本机验证环境：Rust 1.99.1、Node 26.8.1、npm 12.0.2、Tailscale 已运行；Codex CLI 0.167.0（ChatGPT 登录）、Claude Code 2.1.304、opencode、pi 均已安装。`node_modules` 与 `target` 不提交。

### 1.1 ACP 版本与接入事实

- 5.1.0 采用 `Client.v1().connect_with(...)` + 回调模型，而非文档中的 `Client` trait 直接实现；`Harness` trait 仍是边界，SDK 类型不会进入事件模型。
- 子 agent 能力在 `unstable_subagents` feature 后；按 adapter 单独开关，未协商时退化为工具调用卡片。
- `terminal` 声明 `false`；v1 不引入 PTY。
- 真实 Codex adapter 通过 `codex-acp` 启动；由于 npm 的 `@openai/codex` 平台包解析问题，启动时自动把本机 `codex` 路径写入 `CODEX_PATH`（adapter 官方支持的变量）。没有依赖 npx 在运行时下载。
- Zed 的做法是 ACP Registry 安装外部 agent；本项目参考其“由外部进程持有 auth/runtime”的边界，但不复制 Zed 编辑器 GPL 源码。协议层使用 Zed 维护、Apache-2.0 的 `agent-client-protocol` SDK。

---

## 2. 规格冲突裁决

| 冲突 | 裁决 | 依据 |
| --- | --- | --- |
| 05 描述 `!` 内联终端与 Terminals picker，01 非目标与 03 技术栈声明无 PTY / `terminal: false` | v1 不做可交互终端；提供“agent 命令输出”只读面板，终端类工具调用可展开查看完整输出 | 安全边界、单二进制、ACP 能力一致 |
| P2 列“终端面板、文件浏览”，非目标列“不做 IDE” | 文件浏览只读；终端面板只读展示 agent 执行的命令与输出；不提供 shell | 同上 |
| 01 §9 有 Reviews / Merge 相关 thread，02 已定 14 明确不做 | 保留单 thread 内的 Changes 审阅与“合回主工作区”，不做 Reviews/Merge 两类 thread，不做 PR merge 流程 | 02 已定 14 |
| 分享档位在 01 为三档，02 已定 15 改为两档 | 只做“仅邀请 / 整个工作区” | 02 已定 15、17 |
| 通知为 P2（浏览器 + Webhook） | 审批、提问与 @提及都发浏览器通知；Webhook 优先读取设置页 `webhook_url`，环境变量覆盖，契约见 §7 | P2 完整覆盖，不扩成通知平台 |
| ACP v1 RFD 禁止向子会话发 prompt | 子会话无 composer；只提供取消（`can_cancel`）和“让父 agent 转达” | 04 §3.3 |
| 05 的“每个 turn 一个导航刻度”没有可验证样式 | 不做导航轨，保留折叠摘要、时间戳与详情；不写未验证样式 | 05 §4.5 标为 P2/未验证 |

---

## 3. 需求覆盖

状态：`未开始` / `进行中` / `已实现` / `已验证`。只有同时有代码路径和验证证据才标“已验证”。

### 3.1 核心流程 F1–F15

| 需求 | 状态 | 代码路径 | 验证 |
| --- | --- | --- | --- |
| F1 添加并打开 project | 已验证 | `projects.rs`、`routes_fs.rs`、`NewProjectDialog.tsx` | 目录下拉/路径补全；非 git、相对路径、重复路径校验；e2e 打开路径选择器、填入仓库路径并断言 “Use repo” 入口 |
| F2 新建 thread / worktree / agent | 已验证 | `threads.rs`、`workspace/src/git.rs`、`agents.rs`、`WorkspaceLayout.tsx`、`ProjectPage.tsx` | 主按钮点击即建 thread 并跳转，无标题弹窗；相邻 chevron 菜单可选 agent、模型、起始分支；worktree/启动/初始化 prompt；自动标题回归测试；fake + 真实 Codex 完成 turn |
| F3 对话与工具调用 | 已验证 | `agents.rs`、`acp.rs`、`ThreadEvent` 归约、`ThreadTimeline.tsx`、`Composer.tsx` | ACP `ToolCallContent::Diff` 映射为路径/+N/−M/完整 patch，工具卡折叠显示摘要、展开完整 diff；契约测试断言 diff；fake agent 流式文本/思考/工具更新/计划/用量；e2e 显示 “Hello”“Run tests”与 diff 摘要；真实 Codex 返回 `pong` |
| F4 审批 | 已验证 | `approvals.rs`、`routes_actions.rs`、`ThreadTimeline.tsx`、`RightDrawer.tsx` | 默认 pending 不自动放行；按 adapter options 渲染允许/会话/线程/拒绝；审批 DTO 持久化完整 `input_json` 并在内联/抽屉显示 “Full parameters”；内联与抽屉拒绝都要求原因并随 `DecisionRequest.note` 落库；viewer 拒绝；e2e；审计行测试 |
| F5 分享与加入 | 已验证 | `sharing.rs`、`ShareDialog.tsx`、`InvitePage.tsx` | 仅邀请/整个工作区；成员增删改；14 天 token invite 接受/撤销；viewer 只读 e2e |
| F6 多人实时 | 已验证 | `/ws` 事件、`EventStore` 广播、`useThreadLiveEvents` | 双浏览器草稿收敛、审批广播、afterSeq 重放、断线重连测试 |
| F7 变更审阅 | 已验证 | `diff.rs`、`ChangesPanel.tsx`、`routes_workspace.rs`、`git.rs` | 四档基线（含任意 commit 输入）；turn 开始时 snapshot 工作树，Last Turn 基线真实可用；逐文件 hunk、Seen 进度/重置、整体 Revert 到 base、合回不自动提交；thread 与 project 分支分叉时 footer 提供 Keep Local Changes in a New Thread / Discard；e2e 行评论与 Seen；`diff_comment_drift_and_revert_to_base` 验证漂移写回/降级与 revert |
| F8 断线与恢复 | 已验证 | WS 重连、`after_seq` 重放、事件快照、thread 状态、`agents.rs` | afterSeq/replay/resync 测试；浏览器 reload 后消息完整；断线条与重连 UI；loadSession 失败时发 `SystemNotice` 告知上下文已重置 |
| F9 thread 生命周期 | 已验证 | `threads.rs`、`ThreadPage.tsx`、`ProjectPage.tsx` | Thread actions 菜单提供重命名/fork/删除，归档按钮保留；删除二次确认说明 worktree 与未提交变更；e2e 实际 rename → fork → delete 并清理 worktree |
| F10 project 概览 | 已验证 | `projects.rs`、`ProjectPage.tsx`、`WorkspacePage.tsx` | 线程数、运行中、等待审批、增删行数、最近活动；概览计数测试 |
| F11 焦点共享与跟随 | 已验证 | `presence.rs`、`presence.ts`、`TitleBar.tsx`、`Avatar.tsx` | 50ms 合并广播、隐身过滤、在线点、跨 thread 跟随；e2e 跟随 Bob 跳转到第二个 thread 并退出 |
| F12 子 agent 展示 | 已验证 | `SidebarTree.tsx`、`ThreadTimeline.tsx`、`ThreadPage.tsx`、`agents.rs`、`acp.rs`、`WorkspaceLayout.tsx` | 主时间线可展开派遣卡片（状态/cwd/usage/错误原因/无独立会话/打开子会话）、运行中总数提示；ACP `SessionMessage` 生产 `Sent/Received Agent Message` 并持久化；左栏树、嵌套、Show N more、`⌘/Ctrl+J` 全量选择器、面包屑、子视图无 composer；fake `subagents` 与 `agent-message` 测试/截图 |
| F13 提问与回答 | 已验证 | `approvals.rs`、`ElicitationCard.tsx`、`routes_actions.rs`、`agents.rs` | 表单模式；URL 渲染路径已实现但协议能力未启用（`elicitation_url: false`）；三种动作；首个响应获胜；secret 字段拒绝；取消 turn 时 pending elicitation 置 cancelled 并写 `ElicitationResolved`；审计/回归测试 |
| F14 队列与中途插话 | 已验证 | `core/delivery.rs`、`agents.rs`、`routes_agent.rs`、`Composer.tsx` | steering 四种结果；不支持 steering 时默认排队，显式「停止并发送」需二次确认；Stop 取消、队列 tray 显示位置/作者/撤回；取消后 superseded 标记；队列顺序/持久化测试 |
| F15 评论与标注 | 已验证 | `comments.rs`、`ChangesPanel.tsx`、`CommentsPanel.tsx`、`CommentDialog.tsx`、`SelectionComment.tsx`、`ThreadTimeline.tsx` | thread/message/tool/file/diff/elicitation 锚点；选中文本直接评论；文件行评论；回复链；`#message-*` 与 `#event-*` 双向跳转、消息头打开评论区；diff hunk 评论可按 hunk 折叠并保留行 gutter 标记；pending→下一条消息投递及 delivered_seq 跳转；漂移写回 DB 并降级为 File |

### 3.2 功能清单 P0 / P1 / P2

| 优先级 | 功能 | 状态 | 说明 / 证据 |
| --- | --- | --- | --- |
| P0 | project、thread、worktree、启动 agent | 已验证 | 真实临时仓库 + fake/real Codex |
| P0 | 流式消息与工具调用 | 已验证 | 折叠摘要行、工具卡、Thinking、Plan、Usage |
| P0 | 审批（内联 + 收件箱） | 已验证 | 默认拒绝、四种作用域、撤销规则 |
| P0 | 断线重连与事件重放 | 已验证 | seq、afterSeq、resync、snapshot |
| P0 | 分享链接与角色 | 已验证 | 仅邀请/工作区、owner/editor/viewer |
| P1 | Changes 面板与 hunk 审阅 | 已验证 | 四档基线、Seen、行评论、合回 |
| P1 | presence 与跟随 | 已验证 | 语义锚点、50ms 合并、隐身、跟随退出 |
| P1 | archive / fork / delete | 已验证 | 归档列表、worktree 清理、未提交变更提示 |
| P1 | 多 agent | 已验证（结构）/ 环境 smoke | 四条 profile；Codex 真实 smoke；其余 adapter 由 fake 契约与 doctor 覆盖 |
| P1 | 模型与 effort 切换 | 已验证（能力探测） | Composer 通过 `agent-config` 的 Model/ThoughtLevel 下拉切换，`AcpHarness` 用 `session/set_config_option` 应用；New Thread 菜单可预填模型/agent/分支；adapter 未声明时不显示控件 |
| P1 | 多人草稿块 | 已验证 | per-author blocks、块内 Y.Text、他人可编辑、awareness 编辑标记、服务端串行发送 |
| P1 | 焦点共享 | 已验证 | 见 F11 |
| P1 | 子 agent 树 | 已验证 | 左栏嵌套 + tab + 面包屑 + 状态点 |
| P1 | 队列与 steering | 已验证 | FIFO 持久化、四种 steering；不支持 steering 时默认排队、显式停止并发送；Stop 取消、队列条目撤回与 superseded |
| P1 | 评论与标注 | 已验证 | thread/message/tool/file/diff/elicitation 锚点、选区即评论、回复链、pending 批处理、投递跳转、漂移降级 |
| P1 | 提问表单 | 已验证 | accept/decline/cancel、secret 拒绝、第一个回答生效 |
| P2 | project 概览 / 收件箱 | 已验证 | 概览、全局 inbox、未读计数、@提及 |
| P2 | 多 agent 并行对比 | 已验证 | `/p/:projectId/compare` 选择任意两条 thread，并排显示状态、消息数、增删行、审批和最近消息 |
| P2 | 只读文件浏览 / agent 输出 | 已验证 | Files 面板只读树 + 文件内容；Output 面板只读命令输出，无 shell |
| P2 | 子 agent 聚焦 / 对比 | 已验证 | 子 thread 页面 + 面包屑 + 树中节点；`/p/:projectId/t/:threadId/subagents/compare` 并排对照两个子会话；子会话无 composer |
| P2 | URL 模式提问 | 部分实现 | 渲染路径已实现；协议能力未启用（`elicitation_url: false`），显示完整 URL 与主机名、显式点击后新标签页打开、无 iframe/预取的验收待启用后执行 |
| P2 | 通知 | 已验证 | 审批/提问/@提及浏览器通知受 `settings.browserNotifications` 控制；收件箱 5s 轮询覆盖任意页面；请求编辑权限写入 owner/editor 收件箱；最小 Webhook POST（设置项优先） |
| P2 | 终端面板 | 已验证（只读） | 见冲突裁决；不是 PTY |

---

## 4. 代码结构

```
crates/
  shared/     DTO、事件、错误码、ts-rs 导出
  core/       角色/审批策略、投递决策、事件归约（纯逻辑）
  draft/      Y.Doc 草稿块、发送取块、快照
  agent/      Harness trait、AcpHarness、adapter profile、fake agent
  workspace/  路径约束、git worktree、diff、只读文件浏览
  server/     axum、SQLite、REST、WS、CLI、doctor、upgrade、静态资源
web/          React 19 + Vite + Mantine 9 + Yjs/CodeMirror
docs/         规格、实现状态、调研、截图
scripts/      install.sh、e2e-server.sh
```

依赖方向：`server → core → agent | workspace | draft`；`core` 不依赖 axum、sqlx、ACP。事件日志与草稿层分离：事件由服务端 `seq` 定序，草稿走 Yjs CRDT。

---

## 5. 验证证据

### 5.1 质量门

截至最终验证：

```
cargo fmt --all --check                                  pass
cargo clippy --workspace --all-targets -- -D warnings    pass
cargo nextest run --workspace                            314 passed, 1 skipped（审计修复后）
cargo test --workspace                                   pass（含 ignored 的真实 Codex smoke 之外的全部测试）
pnpm --dir web typecheck                                 pass
pnpm --dir web lint                                      pass
pnpm --dir web build                                     pass
pnpm --dir web exec playwright test                      pass（单一 worker，Chromium，43.8s）
```

`cargo nextest` 跳过的 1 个是 `real_codex_streams_a_reply`（标记 `#[ignore]`，需要 Codex 登录）。单独执行：

```
MBX_BLAZE=false cargo test -p threadbase --test real_codex -- --ignored --nocapture
→ TurnEnded { stop_reason: "end_turn" }
→ message_completed text = "pong"
```

### 5.2 端到端

`web/tests/e2e/core.spec.ts` 使用真实 Chrome、真实临时 git 仓库和脚本化 ACP agent，覆盖：

- 首次 setup、admin 登录；project 路径选择器的目录下拉与 “Use repo”；点击 New Thread 直接建线程并跳转，无标题弹窗；相邻 chevron 菜单可设 agent/模型/分支。
- 流式 “Hello”、工具卡 “Run tests”、Plan、Turn ended。
- 内联审批 + 右侧抽屉审批，等待态与处理态。
- reload 后按 seq 重放。
- 第二用户 Bob 以 viewer 打开只读 thread，再提升为 editor。
- 双浏览器共享草稿：Bob 看到 Ada 的块、编辑同一 Y.Text、Ada 看到修改、发送后两侧清空并落库。
- 跨 thread 跟随：Ada 点击 Bob 头像，Bob 切换 thread，Ada 自动跳转并退出跟随。
- Changes：修改 worktree、打开 diff、行内评论、Seen 1/1、文件再变后 Seen 0/1、评论随下一条消息 delivered。
- Thread actions 实际 rename → fork → delete，删除后 project 列表与 worktree 同步刷新。
- 发送 `subagents`：主时间线出现 “Test investigator / No cancel / Nested helper” 可展开卡片，`⌘/Ctrl+J` 打开全量子会话选择器。
- 工具卡显示 ACP diff 摘要（路径 / +N / −N），展开查看完整 patch。
- 审批卡显示 “Full parameters”；拒绝审批时填写原因并提交，确认 `note` 请求路径。
- viewer 点击 `Request access` 后 owner 收件箱出现请求。
- 生成 42 张关键页面截图，覆盖 1440 与 768、深色与浅色；键盘打开 Thread actions 菜单并用 Escape 关闭；浏览器 console error 断言为空。
- 抽屉四个面板均实际打开：Changes、Files、Output、Comments；Compare 与 Subagent Compare 实际路由打开；Thread actions 的 Archive 真正执行并跳回 project。
- 断线：`context.setOffline(true)` 后显示 `Reconnecting…` 并保留输入；恢复网络后重新连接。
- 队列闭环：慢 turn 运行时把 “stream” 入队（Queued at position 1），Stop 取消后条目标记为 previous turn cancelled，点击 continue 触发 queue worker 发送，新的 “Hello” 消息出现且队列清空。
- 分叉：重写临时 project 的 main 分支后打开 thread，footer 出现 `This thread has diverged…`、Keep Local Changes in a New Thread 与 Discard。
- 真实 Codex 的 UI 截图 `composer-real-codex.png` 显示 shadcn Select 的 Model/Reasoning effort 下拉（适配器显示名 `6.1 Sol` / `Max`）。

截图见 `docs/screenshots/`：`workspace-1440-dark/light`、`workspace-768-dark/light`、`settings-1440-dark/light`、`settings-768-dark/light`、`login-1440-dark`、`project-1440-dark`、`project-768-dark/light`、`thread-1440-dark/light`、`thread-768-dark/light`、`composer-1440-dark`、`composer-768-dark/light`、`composer-real-codex`、`changes-1440-dark`、`changes-768-dark/light`、`files-768-dark`、`output-768-dark`、`share-1440-dark`、`share-768-dark/light`、`inbox-1440-dark`、`inbox-768-dark/light`、`subagents-1440-dark`、`subagents-768-dark`、`subagent-picker-1440-dark`、`subagent-compare-1440-dark`、`subagent-compare-768-light`、`compare-1440-dark`、`compare-768-light`、`reconnecting-1440-dark`、`diverged-1440-dark`、`new-thread-menu-1440-dark`。

### 5.3 安全

自动化测试覆盖：

- 服务只接受回环绑定；`THREADBASE_BIND` 非回环直接拒绝。
- argon2id 密码、会话签发/撤销、登出所有设备、登录限流。
- CSRF：自定义头 + Origin；缺任一项 403。
- Tailscale 身份头只在回环 peer 可信；首次 setup 拒绝 Tailscale 请求。
- 路径穿越：词法 + 真实路径 + symlink；超出 project/worktree 拒绝。
- 角色矩阵：viewer 不能写、editor 不能管理、owner 才能分享/删除；admin 例外。
- 审批默认 pending、不自动拒绝、不超时消失；线程级 allow 规则可撤销；子会话 cwd 不在 worktree 下不继承。
- secret 字段拒绝；审计表记录审批/提问/删除/合并/账号变更。

### 5.4 打包与分发

- `MBX_BLAZE=false cargo build --release -p threadbase` 成功（package 名为 `threadbase`，lib 名仍为 `threadbase_server` 以保持内部导入稳定）。
- 在 `/tmp` 运行 release 二进制（不依赖工作区 `web/`），`/api/health`、SPA、`/assets/*` 均 200；把 `web/dist` 临时移走后仍返回 200，证明发布产物使用内嵌资源。
- release 二进制 + fake ACP agent 完整跑通：setup → project → thread → “stream” → tool card → “permission” → 审批 → `permission_resolved`，共 16 条事件。
- 路由级 first prompt 验证：`initialPrompt: "stream"` 创建后 2 秒内产生 10 条事件，队列 worker 已启动 agent。
- viewer 审批绕过回归：`agent_flow::viewer_cannot_unblock_a_tool_through_the_http_route` 返回 403，approval 仍 pending，agent 不产生 completed 工具更新。
- 规则自动放行回归：AllowThread 后第二条同工具请求自动 `permission_resolved`，pending 数为 0。
- `threadbase doctor`：识别 Rust/Node/git、SQLite、四条 adapter（codex/claude/opencode/pi），并给出 `CODEX_PATH`/adapter 信息。
- `threadbase service install --print` 生成 systemd 单元；Linux/macOS 分支实现。
- `threadbase upgrade` 用本地 HTTP 发布的 `.sha256` fixture 验证：下载、SHA256 校验、替换当前二进制、执行新二进制均成功。
- `scripts/install.sh` 通过 `sh -n`；`.github/workflows/release.yml` 覆盖四个目标、上传二进制与 SHA256。
- `scripts/e2e-server.sh` 负责临时仓库、fake agent、web 构建和服务器启动。

---

## 6. 构建加速（mr-boxington + blaze）

本机通过 `mise` 启用 `mr-boxington`，并用 blaze 加速 dev 构建：

```bash
mise use --global --tool-option mr_boxington=true rust mr-boxington
mise use -g github:indexable-inc/blaze@0.20261027.0
mbx settings set blaze true
```

项目在 `Cargo.toml` 中声明：

```toml
[workspace.metadata.blaze]
native-proc-macros = ["dprint-core-macros", "sqlx-macros"]
```

实测：`cargo check --workspace` 走 blaze，冷启动约 25s、热检查约 12s；`cargo test` 走 blaze 的测试 harness 会拒绝部分 std 路径，因此测试统一使用 `MBX_BLAZE=false cargo test` / `cargo nextest`。release、clippy、fake agent fixture 也走普通 cargo，避免 blaze 的实验性限制影响交付。

---

## 7. 通知 Webhook 契约

- 配置：设置页 `webhook_url`；`THREADBASE_WEBHOOK_URL` 作为环境覆盖。不阻塞 thread。
- 事件：`approval.pending`、`elicitation.pending`。
- 请求：`POST`，`Content-Type: application/json`，body `{ event, threadId, summary, createdAt }`。
- 超时 5 秒，失败只写日志；不重试、不做签名队列。
- 浏览器通知：仅在页面隐藏且已授权时发送；设置页可开启，首次请求权限。

---

## 8. 明确不做 / 边界

- M5 relay、跨机同步、Windows、公网暴露、云端沙箱、代码托管、IDE、移动端专用客户端。
- Reviews / Merge 两类 thread、PR merge、per-hunk keep/undo。
- PTY 与 `!` 内联终端；只读 agent 命令输出不是交互 shell。
- 实际发布 GitHub Release、Apple 公证、修改本机 `tailscale serve`/系统服务；仓库只提供脚本、CI 与文档。
- 不自建 agent runtime、不做模型路由；agent 凭据属于机器。

---

## 9. 交互与视觉（2026-11-10 收敛）

以 Linear 官网与 Apple Notes 为目标：一个页面只保留一个主动作，列表用行而不是卡片，说明文字不承担布局职责。

- 添加 project：单个路径输入 + 目录下拉；git 仓库就地带出 “Use <name>”，无名称字段、无说明段落。
- 新建 thread：点击即创建并跳转，不再弹标题表单；标题由首条消息自动生成（“New thread” 时替换为前 60 字），创建后仍可自动启动首次 prompt。
- Composer：per-author block 使用作者色细边、3–10 行自适应高度、最小 152px 输入区；块内可编辑他人草稿；模型/推理强度以下拉呈现，adapter 未提供 configOptions 时完全隐藏。
- Thread 头部只保留 project/thread、状态、审批/队列与归档；branch、模型、agent 等移入状态栏或 Composer。标题栏去掉副标题与 “Search” 文字。
- 左栏是唯一导航；分组和行高收敛，移除口号、营销文案和重复说明。
- 右栏标签改为 shadcn Tabs 图标按钮（Changes / Approvals / Output / Comments / Files），抽屉宽度 360px，内容用 ScrollArea。
- 空状态只留一句动作说明；错误状态说明可操作原因；基础交互统一使用 shadcn 组件，业务组件只做组合。
- 状态矩阵：worktree 准备时显示 `Preparing…` + 步骤 tooltip；路径失效的 project 行提供 `Locate…`；分叉 thread 底部给出 Keep Local Changes in a New Thread / Discard；只读视图提供 `Request access`，不渲染禁用输入框。

设计约束：所有交互必须同时改善 UX（更少步骤）、DX（组件和 API 一致、可测试）与 AX（agent 可通过语义锚点理解和操纵）。不得用装饰性渐变、无意义动画或说明文字弥补布局问题。

## 10. 已知工程注意点

- `ts-rs` 生成的 TS 类型是提交物；修改 Rust DTO 后必须运行 `cargo test -p threadbase-shared --lib` 重新导出。
- 事件与草稿是两套存储：`events` 是权威序列，`draft_updates`/`draft_snapshots` 只服务 Y.Doc。不要跨层写。
- WebSocket 事件通道有 4096 条缓冲；滞后时发送 `resync` 页面。presence 每 50ms 合并一次，不落库。
- 服务端发送草稿时先 `peek_blocks` 非破坏性读取，发送成功后再 `take_blocks` 消费；`send_guard` 按 thread 序列化，两人同时发送只有第一个拿到内容，第二个得到可区分的 `draft already sent` 响应。发送失败不丢草稿。
- 审批规则在 ACP 权限回调到达时匹配 `approval_rules`，命中后自动 `permission_resolved` 并只向 agent 发送 AllowOnce；管理员的 role/scope 校验仍由 HTTP 层先执行。viewer 绕过路由的回归测试在 `crates/server/tests/agent_flow.rs::viewer_cannot_unblock_a_tool_through_the_http_route`。
- 新建 thread 若带 `initialPrompt`，路由在 agent 启动后调用 `kick_queue`，让服务端队列 worker 立即发送，而不是等用户再发一条消息。
- 审批规则在 ACP 权限回调到达时匹配 `approval_rules`，命中后自动 `permission_resolved` 并只向 agent 发送 AllowOnce；HTTP 层先做角色/状态校验。
- 新建 thread 的 `initialPrompt` 在 agent 启动后由 `kick_queue` 立即触发，不再等待用户下一条消息。
- 模型/effort 从 New Thread 传入 `LaunchCtx`，`AcpHarness` 读取 `NewSessionResponse.config_options` 的 Model/ThoughtLevel 并调用 `session/set_config_option`；adapter 未声明时只发 Notice，不伪造能力。
- 子 agent 的 transcript 是否可用取决于 adapter 是否上报子会话事件；未上报时 Compare 页只展示 status/cwd/usage 并明确提示，不伪造 transcript。
- `agent-client-protocol` 5.1 的 unstable 能力可能随 minor 变化；升级前跑 `cargo test -p threadbase-agent` 契约套件和真实 Codex smoke。
- 发送草稿时不删除 Y.Array 中的块，而是清空文本并写 `sentAt` 墓碑。直接删除父级 map 时，客户端并发编辑可能产生悬空 Yjs 更新，yrs 0.17 会在编码时 panic；墓碑模型避免了该问题，Composer 过滤已发送块。
- Codex adapter 需要 `codex` 在 PATH；其他 adapter 通过 `doctor` 检查。真实登录/额度属于环境，不属于仓库。


---

## 11. shadcn/ui 重构（2026-11-10）

- 使用官方 `shadcn@4.23.0` CLI 初始化 `web/components.json`，preset 为 `radix-nova`，base 为 Radix，样式为 Tailwind v4 CSS variables；字体切换到 `@fontsource-variable/geist`。
- 生成并直接使用官方组件：`button`、`badge`、`avatar`、`input`、`textarea`、`checkbox`、`select`、`dialog`、`dropdown-menu`、`tabs`、`tooltip`、`popover`、`command`、`scroll-area`、`sheet`、`switch`、`label`、`separator`、`skeleton`、`input-group`。
- `app-button.tsx`、`app-badge.tsx`、`app-avatar.tsx`、`app-dialog.tsx`、`app-tooltip.tsx` 只保留为应用 API 适配层（loading、业务 label、title/footer 等），内部组合官方组件；旧的 `Button.tsx`、`Input.tsx`、`Dialog.tsx`、`Tabs.tsx`、`Tooltip.tsx` 等手写实现已删除，不再维护两套基础件。
- 语义 token 按 shadcn 约定映射到 Threadbase 的 canvas/panel/surface/raised/border/foreground/muted/primary/accent；深色为默认，`[data-theme="light"]` 完整覆盖；避免 `--muted` 同时承担文本色和 hover 背景的冲突，应用文本统一使用 `muted-foreground`。
- 视觉收敛为 Vercel/Linear 方向：单色 primary 按钮、hairline 分隔、减少卡片边框与阴影、统一 8px 半径、更松的页面留白、Geist 字体和 11–14px 信息层级。
- 核心 e2e 实际打开 Workspace/Project/Thread/Inbox/Settings/Login/Changes/Files/Output/Compare/Subagent Compare，执行 Thread actions 的 Rename/Fork/Delete/Archive、Share、Reconnect 与 Diverged 状态；`CommandPalette`、Composer 模型选择、Changes 基线、Compare 选择器、Share 角色、Elicitation 枚举均使用 shadcn Select/Command/Dialog。
- 后端 API/数据库/事件模型未改动；Inbox 中 “Bob Stone Bob Stone requested edit access” 一类的摘要去重只改 `InboxPage.tsx` 的视图层显示，`request_access` 的原有后端摘要保持不变。
- 仍保留的手写 `<button>` 仅用于 diff 行、文件树、侧栏 treeitem/chevron 等语义化列表控件；所有命令动作使用 `Button`/`IconButton`。
- 验证：`pnpm typecheck`、`pnpm lint`、`pnpm build`、Playwright 1 passed（双浏览器、键盘菜单、1440/768、深/浅色截图）；`cargo nextest` 作为前端改动后的 API 回归。

---

## 12. 统一显式状态模型（2026-11-10）

### 12.1 背景与决策

- 审计：[07-state-machines.md](07-state-machines.md) 登记了 20 个隐式状态机（SM-01..SM-20），指出双真相、重复 parse、死状态、无迁移守卫等问题。
- 决策：[08-state-model-decision.md](08-state-model-decision.md) 选定模型 B（单一写入入口 + 显式迁移表 + UI/agent 共用投影），并给出各实体迁移表、迁移路径与回滚点。

### 12.2 代码落地

- `crates/core/src/state.rs`（纯逻辑，无 IO）：
  - `ThreadMachine`、`ApprovalMachine`、`ElicitationMachine`、`SubagentMachine`、`QueueMachine`、`AgentSessionMachine`、`DeliveryMachine`、`CommentMachine`、`ReadRules`、`PresenceMachine`、`DraftMachine`。
  - 全部状态字符串映射（thread/subagent/queue/approval/elicitation/comment/delivery）集中于此。
- `crates/server/src/state.rs`：`StateStore` 是**所有**生命周期状态列（thread / approval / elicitation / queue / comment / subagent / agent_binding）的唯一写入入口（机器校验 → 落库 → 广播投影）。subagent 的创建与迁移由 `create_subagent` 统一处理；queue retry 经 `QueueMachine::Retry`。
- 创建态写入（thread/approval/elicitation/comment/queue 的 INSERT）也经机器的 `Create`/`Enqueue` 动作校验初始状态，消除了死代码。
- `ThreadState` 投影（`crates/shared/src/api.rs`）：`thread_id / status / current_turn / pending_approvals / pending_elicitations / queued_inputs / active_subagents / diverged`；REST `GET /api/threads/{id}/state`、WS `thread_state`、agent 摘要三者共用同一投影。
- `AppState` 新增 `state_tx` / `state_store`；`ws.rs` 订阅状态广播。
- 后端调用点收敛：`threads.rs`、`ws.rs`、`agents.rs`、`approvals.rs`、`queue.rs`、`routes_agent.rs`、`comments.rs`、`events.rs` 不再各自 parse/status_str；rg 证据见 §12.5。
- `core::reducer` 不再推导 thread status（`TimelineState` 无 status 字段），消除 DB 与事件投影的双真相。
- 前端：新增 `SubagentStatusBadge`，`ThreadPage`/`ThreadTimeline`/`SidebarTree`/`WorkspaceLayout` 不再把 `SubagentStatus` 映射为 `ThreadStatus`；新增 `useThreadState`（REST + WS 投影），`ThreadPage` 用投影的 `status`/`pendingApprovals`/`queuedInputs`/`pendingElicitations`/`currentTurn`/`diverged` 渲染，`ThreadTimeline` 用 `activeSubagentCount` 驱动运行中提示；`live.ts` 的 `thread_state` 校验 threadId 并失效 workspace threads 查询。
- DB：`0003_state_indexes.sql` 为 pending/queue/read 查询补索引；`0001/0002/0003` 均提供 `.down.sql`（已用 sqlite3 验证 up→down 序列）。

### 12.3 修复的不一致

- cancel 取消 elicitation 后 thread 立即回到 idle（原来停留 requires_action）。
- decide/elicitation 后按剩余 pending 数决定 running/requires_action（原来无条件 running）。
- archive 同时写 `archived_at`（原来只写 status）。
- `auto_allow` 对并发决定幂等（原来冲突会使 agent pump 事件循环终止）。
- Pump 事件循环不再因单次状态错误而终止。
- `QueueStatus::Superseded` 真正写入 status；retry 路由接受 `queued/superseded` 并触发 queue worker。
- PromptDone 开始时先清 `session.turn`，消除“完成事件已发布但内存 turn 未清”的竞态（原来并发 send 会误入 steering，导致 subagent 场景不执行）。

### 12.4 AX：agent 语义状态摘要

- 服务端在 prompt 起始从 `ThreadState` 投影生成 `[Thread state] status=…; current_turn=…; pending_approvals=…; pending_elicitations=…; queued_inputs=…; active_subagents=…`，作为附加文本块注入。
- 仅当 pending/queued/active_subagents 非零时注入，避免正常 prompt 噪音。
- 摘要与 UI 共用同一投影，不依赖 adapter 能力；能力缺失时仍准确（降级路径：只报告服务端事实，不伪造能力）。
- 测试：`state::tests::summary_reports_pending_state`（投影内容）、`agent_flow::agent_prompt_includes_state_summary_when_pending`（注入到真实 prompt，读 fake agent journal）。

### 12.5 验证证据

- 质量门：`cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`MBX_BLAZE=false cargo nextest run --workspace`（301 passed, 1 skipped）、`pnpm typecheck`、`pnpm lint`、`pnpm build`、`pnpm exec playwright test`（1 passed）全绿。
- 端到端闭环：同一条 thread 完成 流式 → 审批 → 队列（入队/取消/continue/发送）→ subagent → 断线重连；断线恢复后断言 `Reconnecting…` 消失、消息与工具卡仍在（事件重放）；UI 与后端 bundle 的 pendingApprovals 一致；后端测试断言 agent journal 摘要与 DB pending 计数一致。
- rg 证据：
  - 后端 `fn parse_status|parse_subagent_status|parse_queue_status|parse_approval_status|parse_elicitation_status|parse_comment_status|delivery_str|parse_delivery|status_str|subagent_status_str|comment_status_str` 仅剩 `diff.rs` 的 `FileStatus` parse（与生命周期无关）。
  - `UPDATE … SET status` 在 `crates/server/src` 中只存在于 `state.rs`（StateStore 唯一写入入口）；其他模块无直接状态写入。
  - 前端 `subagentStatus` 映射已删除，仅保留 `SubagentStatusBadge` 与 `ThreadTimeline` 的 `ToolStatus` 单一映射。
- 状态矩阵（空/加载/错误/等待审批/断线/准备中/分叉/只读）保持；e2e 重新生成 40 张关键页面截图并同步到 `docs/screenshots/`（另有 2 张手动/真实 agent 截图，共 42 张）。
- 幂等测试：thread/approval/elicitation/delivery/read/presence/draft/session/subagent/queue/comment 均有同态动作或终态重复拒绝测试。
- 守卫落地：`SubagentMachine::can_cancel` 用于 `cancel_subagent` 拒绝终态子 agent；`QueueMachine::is_terminal` 用于 `retry_queue_item` 拒绝终态队列项。
- 状态矩阵（空/加载/错误/等待审批/断线/准备中/分叉/只读）保持；e2e 重新生成了关键页面截图。

---

## 13. 审计修复（2026-11-10）

依据 [09-audit-findings.md](09-audit-findings.md) 的 P0/P1 清单，本轮落地以下修复。

### 13.1 默认投递改为排队（P0-3）

- `core::delivery::resolve_delivery`：`Auto` 且 turn 运行中、agent 不支持 steering 时返回 `Queue`；显式 `Inject` 但能力缺失时也回退到 `Queue`。`StopAndSend` 仅在调用方显式选择时执行。
- UI：Composer 的「Stop & send」改为两步确认（首次点击显示 `Confirm stop & send`，title 说明会取消当前 turn 与未处理审批）。
- 测试：`running_auto_without_steering_queues`、`explicit_inject_without_steering_falls_back_to_queue`。

### 13.2 草稿发送可恢复且保留多作者（P0-5）

- `DraftDoc::peek_blocks` 提供非破坏性读取；发送流程改为「peek → agents.send → take」，发送失败不消费草稿。
- `DraftHub::send_guard` 按 thread 序列化 peek/send/take，两个发送者不会消费同一批块。
- 空草稿返回可区分的 `draft already sent`（而非 `message cannot be empty`）。
- 多作者保留：`TurnStarted` 事件增加 `authors: Vec<UserBrief>`（`#[serde(default)]` 兼容旧事件）；`AgentManager::send_with_authors` 收集全部草稿块作者；`user_message`、`core::reducer` 与前端 `ThreadTimeline` 同步渲染多作者。
- 排队路径：migration `0006_queue_authors` 给 `queued_inputs` 增加 `authors_json`；`agents::enqueue` 序列化作者列表；`process_queue` 发送时恢复 `authors`（缺省回退到 `[author]`）。
- 本地保留：Composer 在发送前捕获草稿文本，收到 `draft already sent` 时重新写入新块，保证第二个发送者可以重新编辑/重发。
- 测试：`peek_blocks_is_non_destructive`、`turn_started_keeps_multiple_authors`、`turn_started_falls_back_to_primary_author`、`draft_send_preserves_multiple_authors`（Manager 层）、`draft_route_preserves_multiple_authors`（HTTP 路由 + 两份草稿块）、`queued_draft_route_preserves_multiple_authors`（HTTP 路由显式排队 + 队列 worker 恢复作者）。

### 13.3 入站 CRDT 资源约束（P0-6）

- `DraftDoc::limits_ok` 校验单块 16 KB 与总量 200 KB。
- `DraftRoom::apply` 在验证前暂停文档广播：超限更新不广播、不返回持久化；调用方 `drop_room` 丢弃房间，下次从持久化状态重载。
- `DraftHub::is_canonical` 防止被丢弃房间的孤儿连接继续持久化；WebSocket 设置 `max_message_size`/`max_frame_size` 为 256 KB；单个 update 超过 128 KB 跳过。
- 测试：`oversized_update_is_not_broadcast_or_persisted`。
- 未实现：墓碑 GC 与房间空闲淘汰（P2，见 §13.8）。

### 13.4 Seen 与基线解耦（P0-7）

- migration `0004_seen_baseline`：`seen_files` 增加 `baseline` 列，主键变为 `(thread_id, user_id, path, baseline)`；旧数据以空 baseline 迁移（相当于重置 Seen）。
- `diff::mark_seen`/`seen_hashes`/`list` 按解析后的 baseline reference 读写；切换基线不再继承或丢失进度。
- 前端 `SeenRequest` 携带 `baseline`，Changes 面板按当前基线标记。
- 测试：`seen_state_is_scoped_to_baseline`。

### 13.5 current_turn 落库（P1-5）

- migration `0005_current_turn`：`threads.current_turn_id`。
- `AgentManager::prompt` 在 `TurnStarted` 后写入，`TurnEnded`/`TurnError`/`ProcessExited` 清空；队列 worker `process_queue` 在 `TurnStarted` 后同样写入，保证排队 turn 的 `current_turn` 也准确。`ThreadState.current_turn` 与 `AgentManager::current_turn` 改读该列，不再从事件派生。

### 13.6 ThreadState revision（P1-4）

- `ThreadState` 增加单调 `revision`；`StateStore::snapshot_state` 按 thread 串行化，同时为 WS 发布、WS 订阅与 REST 投影分配唯一递增 revision，避免订阅消息与并发发布共用同一 revision。
- 前端 `live.ts` 按 revision 丢弃乱序消息；测试 `snapshot_revisions_are_monotonic` 验证严格递增。

### 13.7 交互键位与设置补全

- Composer 读取 `sendWithModifier` 设置：关闭时 Enter 直接发送，开启时保持 `⌘/Ctrl+Enter`。
- ThreadPage 实现 `⌘W`（归档/取消归档切换）与 `⌘⇧U`（已读/未读切换）；新增 `POST /api/threads/{id}/unread` 端点。
- `⌘/Ctrl+Enter` 也走 `sendWithRetention`，本地草稿保留覆盖全部发送路径（按钮、快捷键、菜单）。

### 13.8 明确不修 / 可接受风险

- **状态与事件不同事务（P1-3）**：`StateStore` 落库与 `EventStore::append` 仍是两个事务；通过 revision + 按 thread 串行广播保证 UI 最终一致，不假设同事务。投影失败已记录 `tracing::warn!`（`publish_thread_state`），崩溃窗口仍需更完整的可观测告警，v1 接受该风险。
- **墓碑 GC 与房间淘汰（P0-6 剩余）**：`sentAt` 墓碑与 `DraftHub.rooms` 未回收；单线程草稿有 200 KB 上限，长期运行需要 GC，v1 不做。
- **热路径（P1-9）**：`projection()` 仍每次调用 `is_diverged`（两次 git 子进程）；`last_seen_at` 仍每请求写；`message_delta` 未合并。v1 接受，后续可缓存/节流。
- **写接口限流（P2-6）**：仅登录限流，写接口未限流；tailnet 可信网络下接受。
- **事件保留策略（P2-3）**：`message_delta` 按 chunk 落库，未做归档压缩；单 thread 10 万事件仍可打开（窗口加载），长期膨胀需保留/导出策略。

---

## 14. 最终验证（2026-11-10 审计修复后）

审计与修复后的完整质量门：

- `cargo fmt --all --check`：pass
- `cargo clippy --workspace --all-targets -- -D warnings`：pass
- `MBX_BLAZE=false cargo nextest run --workspace`：314 passed, 1 skipped
- `pnpm --dir web typecheck`：pass
- `pnpm --dir web lint`：pass
- `pnpm --dir web build`：pass
- `pnpm --dir web exec playwright test`：1 passed（44.0s，Chromium）
- 迁移 `0004_seen_baseline`、`0005_current_turn` 已用 sqlite3 验证 up→down。

新增验证：

- `seen_state_is_scoped_to_baseline`：Seen 按基线隔离，切换基线不继承/丢失。
- `peek_blocks_is_non_destructive`：草稿发送前读取不消费。
- `running_auto_without_steering_queues`、`explicit_inject_without_steering_falls_back_to_queue`：默认投递不再打断 turn。
- `oversized_update_is_not_broadcast_or_persisted`：超限 CRDT 更新不广播、不持久化。
- `draft_send_preserves_multiple_authors`：Manager 层验证 `TurnStarted.authors` 保留多个草稿块作者。
- `draft_route_preserves_multiple_authors`、`queued_draft_route_preserves_multiple_authors`：HTTP 路由级验证立即发送与排队路径都保留多作者。
- `empty_draft_send_returns_already_sent`、`failed_send_retains_draft`：第二个发送者得到 `draft already sent`；发送失败保留草稿。
- `snapshot_revisions_are_monotonic`：WS 订阅/发布与 REST 投影的 revision 严格递增。
- `queued_turn_writes_current_turn`：排队 turn 也写入 `threads.current_turn_id`。
- e2e 覆盖共享草稿、审批、队列、subagent、断线重连、Changes/评论、分享/presence、归档/分叉/删除与 diverged。
- 文档一致性：README 与 docs/01–08 已按 [09-audit-findings.md](09-audit-findings.md) 修正；docs/02 已同步投递优先级、分享档位、子 agent 状态与 URL 模式决策。

真实 agent 依赖：`real_codex_streams_a_reply` 仍为 `#[ignore]`（需要 Codex 登录）；Claude/opencode/pi 依赖 fake 契约与 `doctor`。部署依赖 Rust、Node、git 与 Tailscale；真实 agent 额度与登录属于环境，不属于仓库。

---

## 15. Mantine 迁移（2026-11-10）

- 前端样式底座从内置源码的 shadcn/ui + Radix + Tailwind v4 切换为 Mantine 9（`@mantine/core`、`@mantine/hooks`、`@mantine/notifications`、`@mantine/spotlight`），并移除 `shadcn`、`radix-ui`、`cmdk`、`class-variance-authority`、`clsx`、`tailwind-merge`、`tw-animate-css`、`tailwindcss`、`@tailwindcss/vite`、`sonner`。
- `main.tsx` 使用 `MantineProvider` + `createTheme`（Geist 字体、深色默认、浅色可切）与 `Notifications`；`index.css` 只保留 Mantine styles 与 Geist。
- 删除 `web/src/components/ui/` 的全部 shadcn 组件与 `app-*` 封装；应用级组合（`ErrorState`、`EmptyState`、`LogoMark`、`AvatarStack`）迁移到 `web/src/components/app/`。
- 组件映射：Button→`Button`、IconButton→`ActionIcon`、Badge→`Badge`、Input/Textarea→`TextInput`/`Textarea`/`PasswordInput`、Dialog→`Modal`、DropdownMenu→`Menu`、Select→`Select`、Popover→`Popover`、Tooltip→`Tooltip`、Sheet→`Drawer`、Tabs→`Tabs`、ScrollArea→`ScrollArea`、Skeleton→`Skeleton`、Command→`Spotlight`。
- `toast(...)`（sonner）统一改为 `notifications.show(...)`；`lib/live.ts` 同步。
- 后端 API/数据层/路由路径未改动；e2e 选择器按 Mantine 角色调整（`Password` 精确匹配、队列面板 `data-testid`、菜单焦点等待）。
- 主题：`localStorageColorSchemeManager({ key: "threadbase.theme" })` 作为唯一持久化来源；`index.html` 内联脚本预置 `data-mantine-color-scheme` 避免闪烁；`ThemeBridge` 保留 `toggleTheme`/`setTheme` 兼容层。
- 验证：`pnpm typecheck`、`pnpm lint`、`pnpm build`、Playwright e2e 1 passed；浏览器逐路由核对 10 条路由（/login、/、/p/:id、/p/:id/t/:id、/p/:id/t/:id/s/:id、/p/:id/compare、/p/:id/t/:id/subagents/compare、/inbox、/invite/:token、404）均以 Mantine 深色主题渲染，主题切换 light↔dark 与 subagent 的 `threadbase.theme` 持久化均验证通过。

---

## 16. Tailscale 身份登录（2026-11-10）

部署目标：本机跑服务，只监听 `127.0.0.1:7777`，tailnet 内其他设备经 `tailscale serve` 访问。登录因此收敛为单一方式。

- 移除密码登录：删除 `/api/auth/login`、`/api/auth/setup`、`/api/auth/logout`、`/api/auth/logout-all`、`POST /api/auth/users`，以及 `LoginRequest` / `LoginResponse` / `SetupRequest` / `CreateUserRequest` DTO。
- 移除会话：删除 `auth_sessions` 读写、`issue_session` / `revoke_*` / 会话 Cookie、`argon2` 依赖与密码限流器。
- 认证入口只剩 `CurrentUser` 提取器：读取 `tailscale serve` 转发的 `Tailscale-User-Login` / `-Name` / `-Profile-Pic`，仅信任回环 peer。
- 管理员引导：`THREADBASE_TAILSCALE_ADMIN`（逗号分隔）显式指定；未配置时首个出现的 tailnet 身份自动提升为管理员并写审计 `user.admin_bootstrapped`。
- 开发兜底：`THREADBASE_DEV_IDENTITY`，仅在「回环 + 无 `X-Forwarded-*` / `Tailscale-*` 头」时生效，经 serve 的请求不会命中。
- 前端：登录页改为 Tailscale 身份说明页（角色/错误/引导三态），不再有表单；侧栏「Sign out」改为「Reload workspace」。
- 未知 `/api/*` 路径现在返回 JSON 404，不再回落到 SPA 的 `index.html`。
- 实测 `tailscale serve` 转发头：`Tailscale-User-Login`、`Tailscale-User-Name`、`Tailscale-User-Profile-Pic`、`Tailscale-Headers-Info`、`X-Forwarded-For/Host/Proto`；Host 保留为浏览器侧 host。
- 验证：`cargo fmt --check`、`cargo clippy --workspace --all-targets -D warnings`、`cargo nextest run --workspace`（304 passed, 1 skipped）、`pnpm typecheck`、`pnpm lint`、`pnpm build`、Playwright e2e 1 passed（e2e 通过 `extraHTTPHeaders` 注入身份标头模拟 tailnet）。

---

## 17. 一键 systemd 部署（2026-11-10）

- 新增 `threadbase install`（`crates/server/src/install.rs`）：只做配置，不搬运二进制。`ExecStart` 指向当前进程的可执行路径（`argv[0]`，规范化后），因此不拷贝、不构建、不下载。
- 写入内容：`THREADBASE_BIND`、`THREADBASE_DATA_DIR`、`THREADBASE_PUBLIC_ORIGIN`（由 `tailscale status --json` 的 `Self.DNSName` 推导）、可选 `THREADBASE_TAILSCALE_ADMIN`，以及去重后的 `PATH`（agent CLI 常在 mise/nvm 路径下）。
- 激活：`loginctl enable-linger` + `systemctl --user daemon-reload/enable/restart`，然后 `tailscale serve --bg --https=<port> http://127.0.0.1:<port>`，最后轮询 `/api/health`。
- 默认 `--https-port 8443`，避免覆盖 443 上已有的 `tailscale serve` 配置。
- `--print` 只打印改动；`--uninstall` 停用并删除 unit、关闭对应 `tailscale serve`；二进制与数据目录从不删除。
- 修掉一个真实 bug：`serve_arguments` 漏了 `serve` 子命令，导致实际调用变成 `tailscale --bg ...`（`--print` 因字符串拼接而看不出），已加回归测试。
- 本机实测：`systemctl --user` enabled/active、`Linger=yes`、`MainPID` 的 `/proc/PID/exe` 与 unit 的 `ExecStart` 一致、经 `https://<machine>.ts.net:8443` 自动登录；`--uninstall` 后 unit 与 serve 映射移除。
