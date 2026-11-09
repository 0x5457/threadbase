# 技术栈 v0.2

配套文档：[01-requirements.md](01-requirements.md)、[02-open-decisions.md](02-open-decisions.md)。

v0.4 相对 v0.3 的变化：确认 DeltaDB 基于 CRDT，草稿层改回 CRDT（块内字符级合并），事件层保持定序日志；补充发送事务与确认边界。

---

## 1. 判断标准

「最先进」不等于「最新」。本项目按四条排序取舍：

1. **上游生产验证**：这个库是否被协议作者或头部产品用于同类场景。
2. **协议稳定性**：是否已过 experimental，是否官方维护。
3. **分发匹配度**：本地工具能否做到用户一条命令安装、不折腾运行时。
4. **可维护性**：出错时能否单步调试，能否用测试固定行为。

按这四条，后端选 Rust，而不是因为 Rust 新。

---

## 2. 后端语言选型

### 2.1 关键事实

| 项 | Rust | Go | Node / TS |
| --- | --- | --- | --- |
| ACP 官方 SDK | `agent-client-protocol` 5.0，**Zed 的外部 agent 集成就用它** | 无官方 SDK | `@agentclientprotocol/sdk` 2.4，官方参考实现 |
| 远程传输 | 官方 `agent-client-protocol-http`（HTTP/SSE + WebSocket） | 无 | 无，需自建 |
| 单二进制分发 | 原生 | 原生 | 需要 Node 运行时 + 原生模块 |
| SQLite | `sqlx` / `rusqlite` | `modernc.org/sqlite`（纯 Go）或 cgo | `better-sqlite3` |
| 协同编辑 | `yrs` / `loro` 原生实现 | 只能走 cgo 或 WASM | `yjs` 参考实现 |
| 前端类型同步 | `ts-rs` / `utoipa` | `oapi-codegen` 反向生成 | 天然同语言 |
| 迭代速度 | 慢 | 中 | 快 |
| 可贡献人群 | 少 | 多 | 最多 |

### 2.2 结论

**Rust 作为后端与本地服务的唯一语言。**

三个决定性理由：

1. ACP 的 Rust SDK 是上游作者维护、并且在 Zed 里生产运行的那一份。选择它，等于让协议实现跟着作者走。
2. 本地工具最重要的体验是「装得上」。Rust 产出单个二进制，前端资源用 `rust-embed` 嵌进去；不需要用户为服务端装 Node，也不用担心 `better-sqlite3`、`argon2` 这类原生模块的 ABI 问题。
3. 草稿层要 CRDT（DeltaDB 同样是 CRDT），权威实现在 Rust（`yrs`、`loro`）；Node 方案要靠 WASM 或另一个进程。

### 2.3 代价与对策

| 代价 | 对策 |
| --- | --- |
| 开发速度慢于 TS | 领域逻辑集中在 `core` crate，用 `insta` 快照和 `proptest` 固定行为；前端独立迭代，不阻塞 |
| 编译时间长 | `cargo-nextest`、`sccache`、CI 里拆 debug/release；日常开发用 `cargo check` |
| Rust 人力少 | 核心 crate 保持小而清晰；对外贡献面放在 adapter 配置和前端 |
| ACP crate 版本跳得快 | 锁定 minor 版本；自有 `Harness` trait 隔离；升级必须通过契约测试 |

Go 只在一种情况下会反超：团队完全不会 Rust 且要快速招人。以当前约束（单机、ACP、单二进制）看，不成立。

---

## 3. 总体架构

```
浏览器（多人）
  │ HTTPS / WSS（同源 Cookie 会话 + 二进制通道）
  ▼
threadbase（单个 Rust 进程，axum）
  ├─ REST            /api/*
  ├─ WS              /ws（事件订阅、presence、draft）
  ├─ EventLog        SQLite append-only + 内存环形缓冲 + 快照
  ├─ Drafts          Y.Doc（per-author 块 + Y.Text），CRDT
  ├─ Workspace       git worktree、diff、受限路径访问（notify 监听外部改动）
  └─ Harness         每 thread 一个子进程
        │ stdio（ACP v1 JSON-RPC）
        ▼
    codex-acp → codex app-server → 模型
```

一个进程、一个端口、一个 SQLite 文件。没有 Redis、没有容器、没有 sidecar 服务。

---

## 4. ACP 接入层

### 4.1 用官方 Rust SDK

`agent-client-protocol` 5.0 提供四种角色：Client、Agent、Proxy、Conductor，以及连接构建器和 handler。我们只用 **Client**：

- 启动子进程，接 stdin/stdout，用 v1 连接构建器注册 handler；
- 实现 `Client` trait 的 `session/request_permission`、`fs/read_text_file`、`fs/write_text_file`；
- 声明 `terminal: false`，让 agent 用自己的 shell，v1 不引入 PTY；
- 所有回调都转发到 `core` crate，不让 SDK 类型泄漏到事件模型。

### 4.2 不采用的官方能力，以及原因

| 能力 | 为什么不用 |
| --- | --- |
| `agent-client-protocol-http`（HTTP/SSE + WS） | 它解决「远程 ACP 客户端」，不解决「一个会话多个浏览器」。浏览器若直接当 ACP 客户端，`fs/*`、`terminal/*` 要么暴露给前端，要么全部转发回来，等于多一层；而且会话仍然是 1:1。保留它作为后续「agent 跑在另一台机器」的传输方案。 |
| `Proxy` / `Conductor` 角色 | 适合在编辑器与 agent 之间插入行为。我们的策略、审计、事件归约都在自有 `core` 层，不需要协议级代理链。 |
| ACP v2 | 仍是 draft。v1 的能力（session list / fork / resume / configOptions）已经覆盖 v1 需求。 |

### 4.3 自有 Harness 抽象

```rust
#[async_trait]
pub trait Harness: Send + Sync {
    fn id(&self) -> &'static str;
    async fn start(&mut self, ctx: LaunchCtx) -> Result<Capabilities>;
    async fn new_session(&mut self, req: NewSession) -> Result<SessionId>;
    async fn load_session(&mut self, id: &SessionId) -> Result<()>;
    async fn prompt(&mut self, parts: Vec<Content>) -> Result<TurnHandle>;
    async fn cancel(&mut self, turn: TurnId) -> Result<()>;
    async fn decide(&mut self, req: PermissionId, d: Decision) -> Result<()>;
    fn events(&self) -> broadcast::Receiver<RawAgentEvent>;
}
```

- 默认实现 `AcpHarness`，四家 agent 走同一条路。
- Codex 专属的问题只在 `CodexProfile` 里处理：模式 id（`read-only` / `workspace-write` / `agent` / `agent-full-access`）、`INITIAL_AGENT_MODE` 环境变量、审批语义。
- 预留 `NativeCodexHarness`（直接讲 `codex app-server` 的 JSON-RPC）作为逃生口，但 v1 不实现。理由见 D11。

### 4.4 进程与运行时依赖

- 一条 thread 一个子进程；并发上限默认 3；空闲 15 分钟回收。
- `codex-acp`、`claude-agent-acp`、`pi-acp` 是 npm 包，需要 Node。产品不隐藏这一点：
  - `threadbase doctor` 检查 Node、agent 二进制、登录状态；
  - 安装流程显式展示「将下载 codex-acp 0.16.x」；
  - Claude 的 adapter 提供独立可执行文件，可以不依赖 Node。
- 崩溃重启带退避，最多 3 次；重启不自动续跑上一个 turn，thread 标记为中断。

### 4.5 队列与 steering

投递优先级：**插话 → 停止并发送 → 排队**。前两种是默认路径，排队是显式选择。

- 支持 steering 且 turn 运行中 → 调 `_session/steering`，始终带 `_meta: { steering: { idleBehavior: "promptRequired" } }`：
  - `injected` → 事件 `delivery: injected`；
  - `promptRequired` → 保留输入，交给用户选择「停止并发送」或「排队」；
  - `startedNewTurn` → 视为异常（我们已带 `promptRequired`），记录告警并按新 turn 处理；
  - `failed` → 保留输入并提示。
- 不支持 steering 且 turn 运行中 → 默认 **停止并发送**：`session/cancel` 后立即 `session/prompt`，事件 `delivery: stopped_and_sent`。这是 Delta 的行为。
- 用户显式选「排队」→ 进入 thread FIFO（`VecDeque<PendingInput>`），持久化到 `queued_inputs`，turn 结束后按序发送。
- 队列在 `core`，不在客户端：刷新、换设备、服务重启都不丢。
- 同一会话的 steering 按到达顺序串行，不同会话并行。
- 取消 turn 不清空队列；每条标记 `superseded`，由任一 editor 逐条继续或清空。
- 限制：单 thread 队列上限 50 条，超出拒绝并提示。

---

## 5. 草稿（CRDT）与 presence

事实核对：DeltaDB 明确基于 CRDT（`deltadb.md`：`DeltaDB is built on CRDTs, loosely based on diamond-types and adapted to work with Git`；博客：`as you type, changes sync to your collaborators and to the server`）。但 Delta 的**线程事件**仍是定序日志。

我们采用同样的分层：**文本用 CRDT，事件用定序日志。**

### 5.1 两层模型

| 层 | 内容 | 一致性 |
| --- | --- | --- |
| 事件层 | 消息、工具调用、审批、评论、子 agent 状态 | append-only、服务端 `seq`、可重放 |
| 草稿层 | composer 里的 per-author 草稿块 | CRDT、字符级合并、发送后收敛删除 |

草稿结构：

```rust
// 逻辑结构，实际由 yrs 的 Y.Array / Y.Map / Y.Text 表达
ThreadDraftDoc {
    blocks: Y.Array<DraftBlock>,
}
DraftBlock {
    author: UserId,
    text: Y.Text,
    created_at: i64,
}
```

- 块列表本身也放进 CRDT：并发新建块、删除块、编辑他人块都能合并，不需要额外的 position 服务或 `409` 重试。
- 每个块有自己的 `Y.Text`；两人同时改同一个块时按字符合并，和 DeltaDB 的行为一致。
- 服务端用 `yrs` 读同一份文档，生成消息时直接取末尾连续块。

### 5.2 发送

1. 发送是服务端串行事务：读 Y.Doc → 取末尾连续块 → 生成一条消息事件（`submittedBy` + `authors[]`）→ 从 Y.Doc 删除这些块 → 广播。
2. 两人同时发送：第一个成功，第二个返回 `already_sent`，客户端保留一份本地副本可重新贴回。
3. 删除块是 CRDT 操作，所有客户端自动收敛；不需要额外的「清空」广播。
4. 用量记在发送者头上；agent 看到每条消息的作者。

### 5.3 同步与持久化

- 客户端：`yjs` + `y-codemirror.next`；块内光标与选区用 `y-protocols/awareness`（相对位置，不传像素）。
- 传输：WS 的 `/ws/drafts/:threadId` 二进制通道，承载 y-sync 与 awareness；事件通道保持 JSON，互不干扰。
- 持久化：`draft_updates(thread_id, seq, update BLOB)` 追加；每 2 秒或每 200 次更新写一次 `draft_snapshots`。
- 发送成功后清空该 thread 的 updates 并写空快照。
- 上限：单 thread 草稿总量 200 KB，单块 16 KB；超限告警并拒绝继续增长。
- 重连：客户端带 `state_vector`，服务端回缺失 updates。

### 5.4 边界：不对什么用 CRDT

- **不对事件日志用 CRDT**：消息顺序、审批结果、评论都是服务端定序的离散事实。
- **不对文件内容用 CRDT**：v1 不提供文件编辑器，文件由 agent 通过工具修改。若将来做内置编辑，再评估 DeltaDB 式 CRDT（Git blob 作为 base version）。
- **不把 CRDT 状态写进 `events` 表**：两套存储、两套语义，边界必须清楚。

### 5.5 焦点共享与跟随（presence，不是 CRDT）

「每个人在看什么」是**低频率指针**，不是文本，不合并、不落库。

```rust
pub struct Focus {
    pub user_id: UserId,
    pub project_id: ProjectId,
    pub thread_id: Option<ThreadId>,
    pub panel: Panel,            // transcript | changes | terminal | approvals
    pub target: Anchor,          // 语义锚点
    pub typing: bool,
    pub hidden: bool,
    pub updated_at: i64,
}

pub enum Anchor {
    Message { message_id: MessageId },
    Turn { turn_id: TurnId },
    Diff { file: String, hunk: HunkId, line: u32 },
    File { path: String, line: u32 },
    Draft { block_id: DraftId },
}
```

实现要点：

- 通道：`/ws` 上的 `presence`，JSON，无 `seq`，不重放。
- 订阅粒度是 **project**，不是 thread。否则对方切走 thread 时你收不到事件，跟随就断了。事件通道仍是 thread 粒度。
- 节流：客户端有变化时发送，上限 10 Hz，带尾沿发送；服务端按 `(thread, user)` 只保留最新值，以 20 Hz 合并广播。
- 锚点必须是语义的：绝不传像素坐标和滚动偏移。
- 跟随是纯客户端状态；本地输入、滚动或点击即退出；目标 30 秒无心跳则结束并提示。
- 颜色：`hash(user_id) % palette.len()`，与草稿内 awareness 光标共用同一映射。
- 隐私：`hidden` 为真时服务端不下发该用户焦点；隐身只影响焦点，不影响在线状态。
- 不参与权限：任何基于焦点的行为都必须回到服务端的租约判断。

---

## 6. 事件模型

信封与 v0.1 一致，补两个字段：

```rust
pub struct EventEnvelope {
    pub seq: u64,                          // 线程内单调递增，服务端分配
    pub thread_id: ThreadId,
    pub ts: i64,
    pub actor: Actor,                      // user / agent / system
    pub origin: Origin,                    // local / relay（预留）
    pub subagent_id: Option<SubagentId>,   // 产生该事件的子会话
    pub parent_subagent_id: Option<SubagentId>,
    pub tool_call_id: Option<ToolCallId>,
    pub delivery: Option<Delivery>,        // injected | queued | superseded | dropped
    pub payload: ThreadEvent,
}
```

事件类型（v1）：`turn_started`、`message_delta`、`message_completed`、`tool_call`、`tool_call_update`、`plan_updated`、`permission_requested`、`permission_resolved`、`elicitation_requested`、`elicitation_resolved`、`subagent_updated`、`usage_updated`、`turn_ended`、`turn_error`、`process_exited`、`changes_updated`、`comment_added`、`members_changed`。

- `seq` 在 SQLite 事务内分配，客户端只消费。
- 重连带 `afterSeq`；落后于环形缓冲则 `resync`。
- 流式 `message_delta` 落库时合并（200 ms 或 4 KB），避免单条回复写出上千行。
- 每 500 条或 turn 结束写快照，首屏只读快照 + 增量。

前端 reducer 用 `seq` 幂等；同一 `toolCallId` 做 upsert；`messageId` 做追加。

---

## 7. 存储

SQLite（WAL），`~/.local/share/threadbase/threadbase.db`，`sqlx` + `sqlx::migrate!`。

| 表 | 说明 |
| --- | --- |
| `users` / `auth_sessions` | 账号与可撤销会话（`argon2id`） |
| `projects` | 路径唯一，含 remote 与默认分支 |
| `threads` | 状态、worktree、base_sha、创建者 |
| `thread_members` | owner / editor / viewer |
| `agent_bindings` | agent、adapter 版本、协议版本、session id、cwd、config |
| `events` | 主键 `(thread_id, seq)`，含 `subagent_id` / `tool_call_id` / `delivery` |
| `subagents` | 树形花名册：parent_id、title、status、cwd、can_cancel、usage |
| `queued_inputs` | thread 级队列，服务重启后恢复 |
| `snapshots` | 首屏快照 |
| `draft_updates` | 草稿 Y.Doc 的增量更新（BLOB） |
| `draft_snapshots` | 草稿快照，控制重放开销 |
| `approvals` | 审批审计；不设超时，不自动拒绝 |
| `comments` | 锚点：message / tool_call / elicitation / file / diff_line |
| `attachments` | 元数据入库，文件落盘 |

选择 `sqlx` 而不是 `rusqlite`：编译期 SQL 校验、内置迁移、异步查询。代价是构建需要 `DATABASE_URL`，用 `cargo sqlx prepare` 提交离线缓存。

---

## 8. 实时层

- `axum` 原生 WebSocket，路径 `/ws`，握手时校验会话与 thread 权限。
- 三条逻辑通道共用一个 TCP 连接：

| 通道 | 编码 | 持久化 | 语义 |
| --- | --- | --- | --- |
| `events` | JSON | 是（SQLite） | 有 `seq`，可重放、可 `resync` |
| `presence` | JSON | 否 | 无 `seq`，只保留最新值，可丢 |
| `drafts` | 二进制（y-sync） | 是（updates + 快照） | CRDT，按 `state_vector` 补差量 |

- 客户端发订阅消息 `{subscribe, threadId, afterSeq}`；服务端按订阅推送。
- 背压：单连接待发缓冲超过 1 MB，丢弃 `message_delta` 并下发 `resync`。
- 心跳 30 秒，两次未响应断开；presence 只存内存，不落库、不重放。
- 所有写操作走 REST，WS 只负责下行与 presence/草稿。避免「WS 里再实现一套 RPC」。

---

## 9. 认证与多用户

单机多用户是本产品最容易被低估的攻击面：服务一旦监听非回环地址，访问它就等于能操作这台机器上的代码与 shell。

- 身份优先用 Tailscale：请求经 `tailscale serve` 转发到回环端口，读取其透传的身份头（名称以实测为准）自动建档。
- 本地账号兜底：用户名 + 密码（`argon2id`），用于首次启动、离线、身份头缺失；首个用户为管理员。
- tailnet 用户首次登录默认 `viewer`，管理员提升为 `editor` / `owner`；角色存本地库。
- 会话：`tower-sessions`（DB 存储），Cookie `httpOnly` + `SameSite=Lax` + 生产 `Secure`；支持「登出所有设备」。
- CSRF：写操作校验 `Origin` 与 `Host` 一致，并要求自定义头 `X-Requested-With: threadbase`。
- 限流：登录每分钟 5 次；写接口按 IP 限流（`tower-governor`）。
- 远程访问只走 Tailscale：
  - 服务固定绑 `127.0.0.1:7777`，不提供 `--host 0.0.0.0`。
  - 远程用 `tailscale serve --bg 443 http://127.0.0.1:7777`，由 Tailscale 提供 MagicDNS 域名与证书；前端始终是 HTTPS，Cookie 可稳定使用 `Secure`。
  - 网络层由 Tailscale ACL 控制；应用层仍用账号与 thread 角色，两层都要过。
  - 不实现端口转发、frp、ngrok、内置 TLS；文档明确「不要把端口开到公网」。
  - 身份头只在请求来自回环时读取；服务不绑定非回环地址，避免伪造头。头名称与行为先做最小探针验证，再写进配置。
- 审计：审批、合并、删除 thread、账号变更写审计表，带操作者与来源 IP。

---

## 10. 前端

| 关注点 | 选择 |
| --- | --- |
| 构建 | Vite 8 |
| 框架 | React 19 + TypeScript（`strict`） |
| 样式 | Tailwind v4 + shadcn/ui，深色优先 |
| 路由 | React Router 8（data router），深链接 `/p/:projectId/t/:threadId` |
| 服务端状态 | TanStack Query 5 |
| 实时状态 | Zustand 5 + 自建 reducer（按 `seq` 幂等） |
| 列表 | `@tanstack/react-virtual` 3 |
| Markdown | `react-markdown` 10 + `remark-gfm` + `rehype-sanitize` |
| 代码高亮 | `shiki` 4，进入视口再高亮 |
| Diff | `react-diff-view` 3（hunk widget 挂行内评论）+ `diff` 9 |
| Composer | 每块一个 CodeMirror 6 + `y-codemirror.next` + `yjs`；粘贴图片走 attachments |
| 焦点与跟随 | 自建 `PresenceStore`（Zustand）+ 语义锚点路由；不引入 CRDT |
| agent 树 | 自建 `AgentTree`（扁平化虚拟列表 + 状态点），选中节点过滤时间线 |
| 表单 | `react-hook-form` + `zod` |
| 图标 / 时间 | `lucide-react` / `date-fns` |
| 类型来源 | `ts-rs` 从 Rust 生成 DTO 与事件类型；前端不手写协议类型 |

渲染约定：流式增量用 `requestAnimationFrame` 批量提交；工具卡片按 id upsert；危险操作二次确认并说明后果。

布局：左栏本身就是树（Pinned / Projects → Thread → Subthread，带 chevron 与 `Show N more`），中间时间线，右侧 Changes / Approvals / Members，顶栏固定上下文与在场成员。不设右侧子 agent 面板。具体像素与间距以 `docs/research/delta-ui.md` 的调研结果为准。

---

## 11. Cargo workspace

```
threadbase/
├─ crates/
│  ├─ core/        # 领域：thread、turn、event log、approval、角色判断（无 axum、无 acp）
│  ├─ draft/       # 草稿 Y.Doc：yrs、y-sync、快照（可 feature gate）
│  ├─ agent/       # Harness trait、AcpHarness、进程监督、事件归一化
│  ├─ workspace/   # git worktree、diff、受限路径、notify
│  ├─ server/      # axum、REST、WS、静态资源、迁移、CLI 入口
│  └─ shared/      # DTO、错误码、ts-rs 导出
├─ web/            # React 应用（pnpm + Vite）
├─ docs/
└─ Cargo.toml
```

依赖方向：`server → core → agent | workspace | draft`。`core` 不依赖 `axum`、`agent-client-protocol`、`sqlx`，全部通过 trait 注入，便于用假实现做测试。

---

## 12. 构建与分发

- 产物：`threadbase` 单二进制，内嵌 web 资源（`rust-embed`）；macOS（x86_64 + aarch64）与 Linux（x86_64 + aarch64）各一份。
- 安装：GitHub Releases 附件 + `curl -fsSL ... | sh`；暂不做 Homebrew tap，`cargo install` 作为开发者选项。
- 运行：`threadbase` 启动、打印 URL 与首次登录方式；`threadbase service install` 生成 launchd / systemd user unit。
- 升级：`threadbase upgrade` 从 Releases 拉取对应平台产物并校验 SHA256；数据库迁移在启动时执行，失败则拒绝启动并提示备份路径。
- 发布：tag 触发 CI，四个目标各出产物（macOS aarch64 / x86_64、Linux x86_64 / aarch64），附 SHA256；macOS 若未公证，在 Release 说明里写清 Gatekeeper 的处理方式。
- 运行时依赖：agent adapter 需要 Node（`codex-acp`、`pi-acp`）或独立二进制（`claude-agent-acp`）。`doctor` 明确列出缺什么、怎么装。
- 不做 Docker 镜像，不做 SaaS 部署。

---

## 13. 质量

| 层 | 工具 | 覆盖 |
| --- | --- | --- |
| 单元 | `cargo-nextest` + `insta` 快照 | 事件归约、审批状态机、角色判断 |
| 属性 | `proptest` | 并发事件乱序、重放幂等 |
| 协议 | `agent-client-protocol-test` + 自写 fake agent | 流式、权限、取消、崩溃、非法消息 |
| 集成 | 临时 SQLite + 真实 axum | REST + WS + 重连 |
| e2e | Playwright | 两个浏览器上下文共享 thread、接管、审批 |
| 契约 | 真实 `codex-acp`，标记 `@contract` | CI 不跑，发版前手动跑 |
| 安全 | `cargo-deny`（许可与漏洞）、`cargo-audit` | 依赖治理 |
| 静态 | `clippy -D warnings`、`rustfmt` | 提交前 |

CI 顺序：`fmt → clippy → nextest → build web → playwright → cargo-deny`。

---

## 14. 依赖清单（采样于 2026-02）

| crate | 版本 | 用途 |
| --- | --- | --- |
| `agent-client-protocol` | 5.0 | ACP Client 角色 |
| `axum` | 0.8 | HTTP + WS |
| `tokio` | 1.54 | 异步运行时 |
| `sqlx` | 0.9 | SQLite、迁移 |
| `serde` / `serde_json` | 1 | 序列化 |
| `ts-rs` | 12 | 生成前端类型 |
| `rust-embed` | 8.13 | 内嵌 web 资源 |
| `tower-http` / `tower-sessions` | — | 静态资源、压缩、超时、会话 |
| `tower-governor` | — | 限流 |
| `argon2` | 0.6 | 密码哈希 |
| `tracing` / `tracing-subscriber` | 0.1 | 结构化日志 |
| `notify` | 8.2 | 文件监听 |
| `similar` | 3.2 | diff 计算 |
| `yrs` / `y-sync` | 0.28 / 0.4 | 草稿 CRDT 与同步协议 |
| `axum-extra` | — | typed header、cookie |
| `ulid` | — | 主键 |
| `thiserror` / `anyhow` | — | 错误 |
| `insta` / `proptest` / `cargo-nextest` | — | 测试 |

前端依赖见第 10 节。

---

## 15. 明确不引入

| 不引入 | 原因 |
| --- | --- |
| Node 作为服务端 | 需要运行时与原生模块；单二进制分发失效 |
| Go | 无官方 ACP SDK；协同编辑与原生二进制生态都绕道 |
| `agent-client-protocol-http` | 解决远程 ACP 客户端，不解决多浏览器共享一个会话；保留到跨机器 agent |
| Proxy / Conductor | 策略与审计在自有 `core` 层，不需要协议代理链 |
| ACP v2 | draft，v1 已够 |
| AI SDK `HarnessAgent` / `useChat` | chat 语义装不下审批、计划、diff；且引入 TS 运行时 |
| Next.js | 无 SSR 需求，产物和约定只会增加打包复杂度 |
| Electron / Tauri | v1 纯 web |
| Redis / 消息队列 | 单进程，SQLite + 内存缓冲足够 |
| CRDT 用于事件日志 / 审批 / diff | 这些是服务端定序的离散事实，不是并发文本；只有草稿文本用 CRDT |
| Socket.IO | 自带协议，对本地服务是负担 |

---

## 16. 风险

| 风险 | 对策 |
| --- | --- |
| ACP Rust crate 小版本频繁变动 | 锁 minor；`Harness` trait 隔离；升级跑契约测试 |
| adapter 依赖 Node | `doctor` 检测；Claude 用独立二进制；安装流程显式 |
| Codex `app-server` 标注 experimental | 锁定 adapter 与 codex 版本；异常时提示而非崩溃 |
| 多用户共用一个 agent 登录 | 界面固定展示 agent 身份；审批审计到人 |
| 远程访问变成远程 shell | 默认回环、强制账号、Origin/CSRF、危险操作二次确认 |
| 草稿 CRDT 的复杂度 | 限定在单 thread 的 Y.Doc 内；事件层完全不依赖它；为发送事务写独立并发测试 |
| 事件量增长 | 快照、窗口加载、归档压缩、导出 |
| 无 PTY | v1 声明 `terminal: false`，验证 Codex 不依赖；需要时接 `portable-pty` |

---

## 17. 落地顺序（M1）

1. `cargo` workspace、`core` 事件模型、SQLite 迁移、`ts-rs` 导出。
2. `agent`：`Harness` trait + fake agent fixture，跑通事件归约。
3. `AcpHarness` 接 `codex-acp`：new / prompt / permission / cancel。
4. `server`：axum REST + WS、事件落库、重放、快照。
5. `web`：project 列表、thread 时间线、审批卡片、diff 面板。
6. 账号、登录、权限、两个浏览器同时在线验证。
7. `threadbase doctor`、审计表、thread 元数据。

M1 的验收问题只有一个：**两台电脑打开同一个页面，能否在同一条 thread 里看到同一份流式输出，并各自审批。**

多人草稿块（CRDT）与焦点共享放在 M3；草稿走 y-sync 二进制通道，焦点走 presence JSON。
