# 实现状态与决策记录

配套文档：[01-requirements.md](01-requirements.md)、[02-open-decisions.md](02-open-decisions.md)、[03-tech-stack.md](03-tech-stack.md)、[04-subagents-and-questions.md](04-subagents-and-questions.md)、[05-ui-spec.md](05-ui-spec.md)。

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
| `Tailwind` | v4 | **4.3.5** | 当前稳定 |
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
| 通知为 P2（浏览器 + Webhook） | 浏览器通知 + 应用内收件箱；Webhook 优先读取设置页的 `webhook_url`，环境变量作为覆盖，契约见 §7 | P2 完整覆盖，不扩成通知平台 |
| ACP v1 RFD 禁止向子会话发 prompt | 子会话无 composer；只提供取消（`can_cancel`）和“让父 agent 转达” | 04 §3.3 |
| 05 的“每个 turn 一个导航刻度”没有可验证样式 | 不做导航轨，保留折叠摘要、时间戳与详情；不写未验证样式 | 05 §4.5 标为 P2/未验证 |

---

## 3. 需求覆盖

状态：`未开始` / `进行中` / `已实现` / `已验证`。只有同时有代码路径和验证证据才标“已验证”。

### 3.1 核心流程 F1–F15

| 需求 | 状态 | 代码路径 | 验证 |
| --- | --- | --- | --- |
| F1 添加并打开 project | 已验证 | `crates/server/src/projects.rs`、`routes_workspace.rs`、`web/src/components/workspace/NewProjectDialog.tsx` | 非 git/相对路径/重复添加校验测试；项目概览计数测试；e2e 添加真实临时仓库 |
| F2 新建 thread / worktree / agent | 已验证 | `crates/server/src/threads.rs`、`workspace/src/git.rs`、`agents.rs`、`web/.../NewThreadDialog.tsx` | worktree 创建/清理、fork、归档/删除测试；创建后后台启动 agent；fake + 真实 Codex 均完成 turn |
| F3 对话与工具调用 | 已验证 | `agents.rs`、`ThreadEvent` 归约、`ThreadTimeline.tsx`、`Composer.tsx` | fake agent 流式文本/思考/工具更新/计划/用量；e2e 显示 “Hello”“Run tests”、计划；真实 Codex 返回 `pong` |
| F4 审批 | 已验证 | `approvals.rs`、`routes_actions.rs`、`ThreadTimeline.tsx`、`RightDrawer.tsx` | 默认 pending 不自动放行；允许/拒绝/线程规则/撤销；viewer 拒绝；内联 + 抽屉 e2e；审计行测试 |
| F5 分享与加入 | 已验证 | `sharing.rs`、`routes_comments.rs`、`ShareDialog.tsx`、`InvitePage.tsx` | 仅邀请/整个工作区；成员增删改；14 天 token invite 接受；邀请/撤销测试；viewer 只读 e2e |
| F6 多人实时 | 已验证 | `/ws` 事件、`EventStore` 广播、`useThreadLiveEvents` | 双浏览器草稿收敛、审批广播、afterSeq 重放、断线重连测试 |
| F7 变更审阅 | 已验证 | `diff.rs`、`ChangesPanel.tsx`、`routes_workspace.rs` | 四档基线、逐文件 hunk、Seen 进度/重置、合回不自动提交；e2e 打开 diff、勾选 Seen、重置、添加行评论 |
| F8 断线与恢复 | 已验证 | WS 重连、`after_seq` 重放、事件快照、thread 状态 | afterSeq/replay/resync 测试；浏览器 reload 后消息完整；断线条与重连 UI |
| F9 thread 生命周期 | 已验证 | `threads.rs`、`ThreadPage.tsx`、`ProjectPage.tsx` | 重命名/归档/取消归档/fork/删除与 worktree 清理；归档列表与恢复入口 |
| F10 project 概览 | 已验证 | `projects.rs`、`ProjectPage.tsx`、`WorkspacePage.tsx` | 线程数、运行中、等待审批、增删行数、最近活动；概览计数测试 |
| F11 焦点共享与跟随 | 已验证 | `presence.rs`、`presence.ts`、`TitleBar.tsx`、`Avatar.tsx` | 50ms 合并广播、隐身过滤、在线点、跨 thread 跟随；e2e 跟随 Bob 跳转到第二个 thread 并退出 |
| F12 子 agent 展示 | 已验证 | `SidebarTree.tsx`、`ThreadPage.tsx`、`agents.rs` | 子 thread 树、嵌套缩进、状态、面包屑、子视图无 composer；fake `subagents` 场景写入 `SubagentUpdated`；退化路径见 ACP 测试 |
| F13 提问与回答 | 已验证 | `approvals.rs`、`ElicitationCard.tsx`、`routes_actions.rs` | 表单/URL 两种模式；三种动作；首个响应获胜；secret 字段拒绝；审计测试；e2e 渲染问题卡 |
| F14 队列与中途插话 | 已验证 | `core/delivery.rs`、`agents.rs`、`routes_agent.rs`、`Composer.tsx` | steering 四种结果、停止并发送、排队；队列持久化顺序测试；Composer 的队列/插话菜单与 delivery toast |
| F15 评论与标注 | 已验证 | `comments.rs`、`ChangesPanel.tsx`、`CommentsPanel.tsx`、`CommentDialog.tsx` | thread/diff/message/tool 锚点；pending→下一条消息投递；hunk 评论 e2e；锚点漂移降级为文件级；viewer 只读 |

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
| P1 | 模型与 effort 切换 | 已验证（能力探测） | New Thread 选择 agent/model/effort；`AcpHarness` 从 `NewSessionResponse.configOptions` 读取 Model/ThoughtLevel 并用 `session/set_config_option` 应用；adapter 未声明时发出 Notice |
| P1 | 多人草稿块 | 已验证 | per-author blocks、块内 Y.Text、他人可编辑、awareness 编辑标记、服务端串行发送 |
| P1 | 焦点共享 | 已验证 | 见 F11 |
| P1 | 子 agent 树 | 已验证 | 左栏嵌套 + tab + 面包屑 + 状态点 |
| P1 | 队列与 steering | 已验证 | FIFO 持久化、四种 steering、停止并发送 |
| P1 | 评论与标注 | 已验证 | thread/diff/message/tool 锚点、pending 批处理、漂移降级 |
| P1 | 提问表单 | 已验证 | accept/decline/cancel、secret 拒绝、第一个回答生效 |
| P2 | project 概览 / 收件箱 | 已验证 | 概览、全局 inbox、未读计数、@提及 |
| P2 | 多 agent 并行对比 | 已验证 | `/p/:projectId/compare` 选择任意两条 thread，并排显示状态、消息数、增删行、审批和最近消息 |
| P2 | 只读文件浏览 / agent 输出 | 已验证 | Files 面板只读树 + 文件内容；Output 面板只读命令输出，无 shell |
| P2 | 子 agent 聚焦 / 对比 | 已验证 | 子 thread 页面 + 面包屑 + 树中节点；`/p/:projectId/t/:threadId/subagents/compare` 并排对照两个子会话；子会话无 composer |
| P2 | URL 模式提问 | 已验证 | 显示完整 URL 与主机名，显式点击后新标签页打开，无 iframe/预取 |
| P2 | 通知 | 已验证 | 浏览器 Notification、收件箱、最小 Webhook POST |
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
web/          React 19 + Vite + Tailwind v4 + Yjs/CodeMirror/Radix 风格组件
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
cargo nextest run --workspace                            241 passed, 1 skipped
cargo test --workspace                                   pass（含 ignored 的真实 Codex smoke 之外的全部测试）
pnpm --dir web typecheck                                 pass
pnpm --dir web lint                                      pass
pnpm --dir web build                                     pass
pnpm --dir web exec playwright test                      pass（单一 worker，双浏览器）
```

`cargo nextest` 跳过的 1 个是 `real_codex_streams_a_reply`（标记 `#[ignore]`，需要 Codex 登录）。单独执行：

```
MBX_BLAZE=false cargo test -p threadbase-server --test real_codex -- --ignored --nocapture
→ TurnEnded { stop_reason: "end_turn" }
→ message_completed text = "pong"
```

### 5.2 端到端

`web/tests/e2e/core.spec.ts` 使用真实 Chrome、真实临时 git 仓库和脚本化 ACP agent，覆盖：

- 首次 setup、admin 登录、创建 project/thread。
- 流式 “Hello”、工具卡 “Run tests”、Plan、Turn ended。
- 内联审批 + 右侧抽屉审批，等待态与处理态。
- reload 后按 seq 重放。
- 第二用户 Bob 以 viewer 打开只读 thread，再提升为 editor。
- 双浏览器共享草稿：Bob 看到 Ada 的块、编辑同一 Y.Text、Ada 看到修改、发送后两侧清空并落库。
- 跨 thread 跟随：Ada 点击 Bob 头像，Bob 切换 thread，Ada 自动跳转并退出跟随。
- Changes：修改 worktree、打开 diff、行内评论、Seen 1/1、文件再变后 Seen 0/1、评论随下一条消息 delivered。
- 1440/768、深色/浅色截图，浏览器 console error 断言为空。

截图见 `docs/screenshots/`。

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

- `MBX_BLAZE=false cargo build --release -p threadbase-server` 成功。
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

## 9. 已知工程注意点

- `ts-rs` 生成的 TS 类型是提交物；修改 Rust DTO 后必须运行 `cargo test -p threadbase-shared --lib` 重新导出。
- 事件与草稿是两套存储：`events` 是权威序列，`draft_updates`/`draft_snapshots` 只服务 Y.Doc。不要跨层写。
- WebSocket 事件通道有 4096 条缓冲；滞后时发送 `resync` 页面。presence 每 50ms 合并一次，不落库。
- 服务端发送草稿时由 `DraftHub::take_blocks` 串行取块；两人同时发送只有第一个拿到内容，第二个返回空消息校验错误。
- 审批规则在 ACP 权限回调到达时匹配 `approval_rules`，命中后自动 `permission_resolved` 并只向 agent 发送 AllowOnce；管理员的 role/scope 校验仍由 HTTP 层先执行。viewer 绕过路由的回归测试在 `crates/server/tests/agent_flow.rs::viewer_cannot_unblock_a_tool_through_the_http_route`。
- 新建 thread 若带 `initialPrompt`，路由在 agent 启动后调用 `kick_queue`，让服务端队列 worker 立即发送，而不是等用户再发一条消息。
- 审批规则在 ACP 权限回调到达时匹配 `approval_rules`，命中后自动 `permission_resolved` 并只向 agent 发送 AllowOnce；HTTP 层先做角色/状态校验。
- 新建 thread 的 `initialPrompt` 在 agent 启动后由 `kick_queue` 立即触发，不再等待用户下一条消息。
- 模型/effort 从 New Thread 传入 `LaunchCtx`，`AcpHarness` 读取 `NewSessionResponse.config_options` 的 Model/ThoughtLevel 并调用 `session/set_config_option`；adapter 未声明时只发 Notice，不伪造能力。
- `agent-client-protocol` 5.1 的 unstable 能力可能随 minor 变化；升级前跑 `cargo test -p threadbase-agent` 契约套件和真实 Codex smoke。
- 发送草稿时不删除 Y.Array 中的块，而是清空文本并写 `sentAt` 墓碑。直接删除父级 map 时，客户端并发编辑可能产生悬空 Yjs 更新，yrs 0.17 会在编码时 panic；墓碑模型避免了该问题，Composer 过滤已发送块。
- Codex adapter 需要 `codex` 在 PATH；其他 adapter 通过 `doctor` 检查。真实登录/额度属于环境，不属于仓库。
