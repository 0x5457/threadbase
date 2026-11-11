# 07 · 隐式状态机审计

> **已被取代**：本文件是「统一显式状态模型」决策（[08-state-model-decision.md](08-state-model-decision.md)）的输入。08 §10 已记录 P1–P2 落地与修复；以下内容仅作历史审计，不代表现状。

## 1. 目的与范围

本文件登记 Threadbase v1 中所有隐式状态机，作为「统一显式状态模型」决策（下一阶段）的输入。

范围：

- 核心会话簇：thread/turn、approval、elicitation、subagent、queue/delivery、agent session、tool/plan。
- 次要机器：presence/connection、draft、inbox、changes-seen、comment、auth/invite、project/diverged、通知。

方法：从 `crates/shared` 的状态枚举与 DTO 出发，用 `rg` 追踪每个状态值的写入（SQL `UPDATE` / 事件 append / CRDT 操作）与读取（parse / match / 派生），再对照前端映射。行号基于当前工作区（2026-11-10）。

---

## 2. 状态机清单

### SM-01 ThreadStatus（线程状态）

- 权威源：`threads.status`（SQLite 列）；枚举 `crates/shared/src/model.rs:44`。
- 状态：`preparing` / `idle` / `running` / `requires_action` / `interrupted` / `error` / `archived`。
- 迁移与触发：

| 触发 | 目标状态 | 位置 |
| --- | --- | --- |
| create | idle | `threads.rs:189` |
| restart | preparing | `agents.rs:119` |
| ensure_started 完成 | idle | `agents.rs:268` |
| prompt 成功 | running | `agents.rs:436`、`agents.rs:1027` |
| decide 成功 | running（无条件） | `agents.rs:501` |
| auto rule 命中 | running | `agents.rs:918` |
| PermissionRequest | requires_action | `agents.rs:690` |
| ElicitationRequest | requires_action | `agents.rs:722` |
| PromptDone | idle | `agents.rs:830` |
| Error | error | `agents.rs:843` |
| ProcessExited | interrupted | `agents.rs:855` |
| mark_agent_error | error | `agents.rs:1072` |
| archive / unarchive | archived / idle | `threads.rs:263-272` |

- 写入者：`threads::create/update`、`Inner::set_thread_status`（`agents.rs:554`）、`Pump::set_status(&str)`（`agents.rs:935`）、多处裸 SQL。
- 读取者：`threads::parse_status`（`threads.rs:637`）、`ws::parse_status`（`ws.rs:204`）、`agents::status_str`（`agents.rs:1223`）、`core::reducer`（`reducer.rs:91/189/218/224/260/269/274/297`）、前端 `ThreadStatusBadge`、`ThreadPage`、`WorkspaceLayout`、`ProjectPage`。
- 问题：
  - 同一状态有 3 个 parse、1 个 status_str、1 个 reducer 派生，共 5 处转换逻辑。
  - DB 状态与 reducer 派生状态是两套并行真相：reducer 没有 archived 迁移；DB 的 `requires_action` 在取消 elicitation 后不会回退（`agents.rs` cancel 路径不设状态）。
  - `decide` 无条件置 running（`agents.rs:501`），与 reducer 的「无 pending 才 running」（`reducer.rs:216`）冲突；多审批并发时 DB 与事件投影不一致。
  - `Pump::set_status(&str)` 接受任意字符串，无迁移守卫；archived/error 可被后续事件覆盖。
  - `preparing` 只在 restart 写入；create 直接 idle，UI 的 `Preparing…` 场景只在重启时出现。

### SM-02 Turn / Session Run（回合与运行态）

- 权威源：内存 `Session.turn: Option<TurnHandle>` + `RunState`（`agents.rs:40-56`）；事件 `turn_started` / `turn_ended` / `turn_error`；`threads.last_turn_sha`。
- 状态：idle（无 turn）→ running（`Some(handle)` + `turn_id`）→ ended / error / interrupted。
- 迁移：
  - prompt → `session.turn = Some(handle)` + `RunState.turn_id`（`agents.rs:428-434`、`1008-1016`）。
  - PromptDone → 清空 `RunState`（`agents.rs:795-803`），但**不清** `session.turn`。
  - Error / ProcessExited → 不清 `RunState`（`agents.rs:834-856`）。
  - cancel → `session.turn.take()`（`agents.rs:466`）。
  - process_queue：非 running 时清 `session.turn`（`agents.rs:966-969`）。
- 读取者：`send()` 用 `session.turn.is_some() && row.status == "running"`（`agents.rs:289`）；`current_turn()` 从 events 表查最新 `turn_started`（`agents.rs:562`）；reducer 的 `current_turn`；前端从事件流推导。
- 问题：
  - 同一次 turn 的状态分散在 4 处：`Session.turn`、`RunState.turn_id`、DB `threads.status`、事件流；无单一 turn 机。
  - PromptDone 后 `session.turn` 仍为 `Some`，只能靠 DB status 判 idle；`send` 的 running 判定是内存与 DB 的混合。
  - `current_turn()` 每次查库取最新 `turn_started`，与 `RunState` 可能不一致（注入消息场景）。
  - `turn_error` / `process_exited` 不清 `RunState`，残留 `turn_id`。

### SM-03 Delivery / Steering（投递与 steering）

- 权威源：`core::delivery::resolve_delivery`（`delivery.rs:35`）；`DeliveryMode`（`api.rs:124`）→ `SendAction` → `Delivery`（`model.rs:79`）。
- 状态：
  - `DeliveryMode{auto, inject, stop_and_send, queue}`；
  - `SendAction{prompt, inject, stop_and_send, queue}`；
  - `Delivery{injected, stopped_and_sent, queued, dropped}`；
  - `SteeringOutcome{injected, startedNewTurn, promptRequired, failed}`。
- 迁移：`resolve_delivery` 纯函数（`delivery.rs:35-68`）；steer → `injected`/`startedNewTurn`（视为 injected，`agents.rs:325-340`）→ `promptRequired`（转 queued，`agents.rs:341-346`）→ `failed`（错误，`agents.rs:347-350`）；queue → queued。
- 读取者：`agents.rs:281-370`、`events.rs:339-353`（delivery parse）、前端 `Composer` / `SendMessageResponse`。
- 问题：
  - 三个枚举表达同一决策链，概念重叠（`DeliveryMode` vs `SendAction` vs `Delivery`）。
  - `Delivery::Dropped` 从未被构造（dead state）；`events.rs:353` 只会解析出它。
  - `StartedNewTurn` 被静默当作 injected（文档标注为异常），没有独立状态。
  - 队列的 `Delivery::Queued` 与 `QueueStatus::Queued` 同名不同义。

### SM-04 ApprovalStatus（审批状态）

- 权威源：`approvals.status`（SQLite）；事件 `permission_requested` / `permission_resolved`。
- 状态：`pending` / `allowed` / `denied` / `cancelled`。
- 迁移：

| 触发 | 目标状态 | 位置 |
| --- | --- | --- |
| create | pending | `approvals.rs:149` |
| decide（AllowOnce/Session/Thread） | allowed | `approvals.rs:227-247` |
| decide（Deny） | denied | `approvals.rs:227-247` |
| auto_allow（规则命中） | allowed | `approvals.rs:940` |

- 写入者：`approvals::decide`、`approvals::auto_allow`。
- 读取者：`approvals::parse_status`（`approvals.rs:94`）、reducer（`reducer.rs:188/208/210/216`，从 decision 反推 status）、前端审批卡 / `ThreadPage`。
- 问题：
  - `Cancelled` 从未写入（dead state）；reducer 用 decision 反推 status，而不是信任 DTO 的 status 字段。
  - 审批规则（`approval_rules`）是子状态机：scope `once/session/thread`，匹配优先级未显式建模；`auto_allow` 与 `decide` 都会改状态。
  - 多审批并发时 DB 状态与 reducer 状态可能不一致（见 SM-01）。
  - pending 计数在 `threads.rs:493`、`ws.rs:190`、`approvals.rs:51` 三处重复查询。

### SM-05 ElicitationStatus（提问状态）

- 权威源：`elicitations.status`；事件 `elicitation_requested` / `elicitation_resolved`。
- 状态：`pending` / `accepted` / `declined` / `cancelled` / `expired`。
- 迁移：

| 触发 | 目标状态 | 位置 |
| --- | --- | --- |
| create | pending | `agents.rs:708` |
| respond（Accept/Decline） | accepted / declined | `approvals.rs:403-411` |
| cancel_pending_elicitations | cancelled | `agents.rs:58-64` |

- 读取者：`approvals.rs:602-606`、reducer（`reducer.rs:240-246`）、前端 `ElicitationCard`。
- 问题：
  - `Expired` 从未写入（dead state）；无超时机制。
  - `blocking` 与否不改变状态机；取消后 thread 状态不回退（见 SM-01）。
  - reducer 从 action 反推 status。

### SM-06 SubagentStatus（子 agent 状态）

- 权威源：`subagents.status`（SQLite）；事件 `subagent_updated`；ACP `StateUpdate` 映射（`crates/agent/src/acp.rs:1346-1363`）。
- 状态：`running` / `requires_action` / `idle` / `cancelled` / `error`。
- 迁移：adapter `SubagentEvent` → upsert（`agents.rs:727-742`）；`cancel_subagent` 只调 harness（`agents.rs:528`），状态由 adapter 后续事件驱动。
- 读取者：`threads::parse_subagent_status`（`threads.rs:877`）、`agents::subagent_status_str`（`agents.rs:1213`）、reducer upsert、前端 `SidebarTree.tsx:470-477`（映射为 ThreadStatus）、`ThreadTimeline.tsx:406/818-850`、`WorkspaceLayout.tsx:647`、`SubagentComparePage.tsx:106`。
- 问题：
  - 前端把 `SubagentStatus` 映射成 `ThreadStatus`（`SidebarTree.tsx:470`、`ThreadPage.tsx:70-77`），两套状态互相转换，语义丢失（cancelled → idle/interrupted）。
  - 服务端无迁移守卫，完全信任 adapter 上报；`Unknown` → idle。
  - `can_cancel` 是独立布尔状态，与 status 无关联校验。
  - 子 agent 的 `requires_action` 不会驱动 thread 状态（thread 只在主 session 的 PermissionRequest/ElicitationRequest 时进入 requires_action）。

### SM-07 QueueStatus / queued_inputs（队列状态）

- 权威源：`queued_inputs.status` + `superseded` 布尔 + `position`。
- 状态：`queued` / `sending` / `sent` / `superseded` / `dropped`。
- 迁移：

| 触发 | 目标状态 | 位置 |
| --- | --- | --- |
| enqueue | queued | `agents.rs` enqueue、`queue.rs` enqueue |
| process_queue 取件 | sending | `agents.rs` process_queue |
| prompt 成功 | sent | `agents.rs` process_queue |
| prompt 失败 | dropped | `agents.rs` process_queue |
| 用户删除队列项 | dropped | `routes_agent.rs` |
| mark_superseded | superseded（status + 布尔） | `queue.rs` / StateStore::supersede_queue |
| retry（Composer continue） | queued | `routes_agent.rs` / StateStore::retry_queue_item |

- 读取者：`queue.rs` dto、`routes_agent.rs` dto、reducer、前端队列面板 / `ThreadPage`。
- 问题：
  - `superseded` 既是状态枚举又是布尔列；现在 status 与布尔同时写入。
  - `dropped` 同时用于队列项和 `Delivery`（`Delivery::Dropped` dead）。
  - 队列 worker 用 `session.turn` + `thread_status` 判断是否可发，与 SM-01/SM-02 耦合。
  - `position` 用 `MAX(position)+1`，无并发保护（单 worker 串行，但 enqueue 与 worker 可能竞争）。
  - 终态：`sent/dropped`；`superseded` 可经 retry 回到 `queued`。

### SM-08 Agent session / binding（会话与绑定状态）

- 权威源：内存 `AgentManager.sessions: HashMap<thread_id, Session>`（`agents.rs:33`）；`agent_bindings.status`（SQLite）。
- 状态：会话 started/stopped；binding status `idle`/`error`；`Session.turn`；`Capabilities`（steering/subagents）。
- 迁移：`ensure_started` → 插入 sessions + binding idle（`agents.rs:220`）；`restart` → 移除 + preparing + ensure_started（`agents.rs:117-122`）；`mark_agent_error` → binding error（`agents.rs:1077`）；进程退出 → interrupted。
- 读取者：binding status **从未被读取**（只写）；sessions 内存是真正状态。
- 问题：
  - `agent_bindings.status` 是只写状态（idle/error），无读取者，且与 `threads.status` 重复。
  - 会话存活状态只在内存，重启服务后 binding status 仍是 error/idle，与实际不符。
  - `Capabilities` 未持久化，UI 轮询 `agentConfig`。

### SM-09 ToolStatus / PlanStatus（工具与计划状态）

- 权威源：`ToolCallData.status`（事件 upsert）；`PlanEntry.status`（整表替换）。
- 状态：`ToolStatus{pending, in_progress, completed, failed}`；`PlanStatus{pending, in_progress, completed}`。
- 迁移：adapter `ToolCall`/`ToolCallUpdate` → upsert（`reducer.rs:155-160`）；Plan → 全量替换（`reducer.rs:162`）。
- 读取者：`ThreadTimeline.tsx:532`（toolStatusVariant）、Plan 面板。
- 问题：
  - 服务端无迁移校验；状态由 adapter 字符串映射（`acp.rs:1172`）。
  - Plan 是「最新快照」而非持久状态机；无 per-entry 迁移。
  - `ToolCall` 与 `ToolCallUpdate` 都走 upsert，事件类型无语义区分。

### SM-10 Presence / focus / connection（在线与连接状态）

- 权威源：内存 `PresenceHub`（`presence.rs`）；WS 连接；客户端 zustand stores。
- 状态：在线/离线；`typing`；`hidden`；`following`（客户端）；WS `connected`/`disconnected`；Composer provider `connecting`/`connected`/`synced`。
- 迁移：WS presence 消息 → `update`；断开 → `remove_user`（`presence.rs:106`）；客户端 `useLiveStore.connected`（`live.ts:12/101/228`）；`useOnlineStatus`（`hooks.ts:26`）；`usePresenceStore.clear`。
- 读取者：`ws.rs`、`presence.rs`、`web/src/lib/live.ts`、`web/src/lib/presence.ts`、`StatusBar.tsx:91`、`WorkspaceLayout.tsx:290`、`Composer.tsx:93-126`。
- 问题：
  - 多条独立 WebSocket（事件 `/ws`、presence 复用 `/ws`、草稿 `/ws/drafts`）→ 连接状态分散在 `liveConnected`、presence store、composer status。
  - `following` 是纯客户端状态，30s 超时，无服务端锚点。
  - `hidden` 只影响下发，不影响在线；presence 不落库、不重放，断线即丢。
  - 连接状态没有统一机器：浏览器 online、WS connected、presence synced、draft provider synced 各自为政。

### SM-11 Draft（草稿状态）

- 权威源：Y.Doc（CRDT）+ `draft_updates` / `draft_snapshots`；Composer provider status。
- 状态（每块）：editing → `sentAt` 墓碑（已发送）；provider：`connecting`/`connected`/`synced`；发送：optimistic。
- 迁移：创建块 → 编辑（CRDT）→ 发送（`take_trailing` / `sentAt`）；清空（`drafts.rs:130-137`）。
- 读取者：`Composer.tsx:120-176`、`drafts.rs`。
- 问题：
  - 状态分散在 Y.Doc、`DraftHub`、provider status、`sentAt` 墓碑四处，无显式机器。
  - 两人同时发送的竞争靠「第二个拿不到块」隐式处理（`docs/06` §10）。
  - 发送中/失败重试状态未建模；`from_draft` 与 `text` 两条发送路径。

### SM-12 Inbox / thread read（已读状态）

- 权威源：`inbox.read_at`；`thread_reads.last_read_seq`。
- 状态：read/unread；thread unread（`max_seq > last_read`）。
- 迁移：`mark_inbox_read`（全量/单 thread，`approvals.rs:774-790`）；`mark_read`（`routes_agent.rs:261-277`）；新事件 → unread。
- 读取者：`ws.rs:32`（InboxCount）、`approvals.rs:698`、`threads.rs:503-539`、`InboxPage.tsx`、`ThreadDto.unread`。
- 问题：
  - 两套已读标记语义不同：`inbox.read_at` 针对通知项，`thread_reads.last_read_seq` 针对事件进度；UI 同时展示。
  - 已读状态无事件，客户端靠 `invalidateQueries` + 轮询；WS 只推 `inbox_count`。
  - thread unread 每次 dto 计算（`MAX(seq)` 查询）。

### SM-13 Changes seen（变更已读）

- 权威源：`seen_files(thread_id, user_id, path, content_hash, seen_at)`；baseline（客户端 `useState`）。
- 状态：seen/unseen；baseline kind `{base_branch, last_commit, last_turn, commit}`。
- 迁移：`mark_seen(true/false)`（`diff.rs:292-325`）；content_hash 变化 → 删除 seen 行（`diff.rs:130`）。
- 读取者：`diff.rs` stats / `seen_hashes`、`ChangesPanel.tsx:118-198`、`StatusBar.tsx:101-113`、`ThreadDto.changes`。
- 问题：
  - seen 与 baseline 耦合：切换 baseline 时 seen 语义变化，但 `seen_files` 不按 baseline 区分（按 path + content_hash）。
  - baseline 是客户端临时状态，未持久化；默认值在 settings。
  - seen 计数在多处派生（`ThreadDto.changes`、`StatusBar`、`ChangesPanel`）。

### SM-14 CommentStatus（评论状态）

- 权威源：`comments.status`。
- 状态：`draft` / `pending` / `delivered` / `resolved`。
- 迁移：`create(send=false)` → pending；`create(send=true)` → delivered（`comments.rs:166`）；`mark_delivered` → delivered（`comments.rs:278`）；`update` → 任意 status（`comments.rs:231`）。
- 读取者：`comments.rs:138-141`、reducer、`CommentsPanel.tsx:82/172/203-215`、`ChangesPanel.tsx:463`。
- 问题：
  - `draft` 只能经 `update` 写入，`create` 不产生 draft（semi-dead）。
  - `update` 接受任意 status，无守卫；`delivered_seq` 与 status 并行。
  - reducer 只 upsert，不校验迁移。

### SM-15 Auth / session（认证状态）

- 权威源：cookie session + `auth_sessions`；`needs_setup` 全局（`auth.rs:168-183`）。
- 状态：`needs_setup` / logged_out / logged_in；`is_admin`；`hidden_presence`。
- 迁移：`setup_admin`、login、logout、logout_all、session 撤销。
- 读取者：`auth_status`、`me`、middleware、`App`。
- 问题：
  - 状态是请求时计算的 DTO，无持久状态机；session 失效靠中间件。
  - `needs_setup` 是全局锁，无显式迁移事件。

### SM-16 Invite（邀请状态）

- 权威源：`invites.accepted_at` / `expires_at`。
- 状态：pending / accepted / expired / revoked（deleted）。
- 迁移：create → pending；accept → accepted（`sharing.rs:284`）；revoke → delete（`sharing.rs:477`）；过期在 accept 时检查（`sharing.rs:271`）。
- 问题：过期无后台推进，状态靠读取时判断；无 status 列。

### SM-17 Project available / thread diverged（派生状态）

- 权威源：每次请求计算（`projects.rs:44` `path_available`；`threads.rs:546` `is_diverged`）。
- 状态：`available` true/false；`diverged` true/false。
- 问题：派生状态无缓存/事件；每次 dto 触发 git `rev-parse`；`diverged` 驱动 UI banner 与 merge-back/revert，但没有显式状态机。

### SM-18 ThreadBundle / UI 派生状态

- 权威源：REST bundle（`routes_workspace.rs` thread_bundle）+ WS events + WS `thread_state`（`ws.rs:184-213`）+ 轮询（`queries.ts`）。
- 状态：UI 的 `runningCount`、`waitingCount`、status badge、pending/queued 计数。
- 问题：同一状态有 4 个来源；前端多处自行映射（`ThreadStatusBadge`、`SidebarTree` → ThreadStatus、`ThreadPage` subagentStatus、`WorkspaceLayout.tsx:134-145`、`ProjectPage.tsx:182-190`）；`docs/05` §13 状态矩阵靠各页面自行实现。

### SM-19 Agent capabilities / config

- 权威源：内存 `Capabilities`；`agent_bindings` config；settings `default_agent/model`。
- 状态：steering supported；subagents supported；model/effort。
- 问题：能力未持久化/未事件化；UI 轮询 `agentConfig`；能力缺失时退化路径散落在 `agents.rs`。

### SM-20 Notification / webhook

- 权威源：`settings.browser_notifications`；Notification permission；`webhook_url`。
- 状态：permission granted/denied；webhook 配置。
- 问题：与 core 状态耦合（approval/elicitation pending 触发），但无统一「通知状态」机器。

---

## 3. 共性问题

1. **一个状态，多处转换**：ThreadStatus 有 3 个 parse + 1 个 status_str + 1 个 reducer 派生 + 多处裸 SQL；QueueStatus 有 3 个 parse；SubagentStatus 有 2 个 parse + 前端映射。
2. **双真相**：DB 状态与事件投影状态并行（thread status、approval status、elicitation status）；reducer 从 decision/action 反推，而不是信任 DTO。
3. **死状态**：`Delivery::Dropped`、`ApprovalStatus::Cancelled`、`ElicitationStatus::Expired`、`QueueStatus::Superseded`（作为 status 列）、`CommentStatus::Draft`（create 路径）、`agent_bindings.status` 的 running 语义。
4. **无迁移守卫**：所有状态写入都是裸 `UPDATE`，没有合法迁移表；非法迁移（如 archived → running、error → idle）不会被拒绝。
5. **取消/失败不回退**：cancel 取消 elicitation 后 thread 仍 `requires_action`；turn error 后 `RunState` 残留。
6. **派生状态重复计算**：unread、diverged、seen、pending 计数在多处查询；前端再派生一次。
7. **客户端状态碎片化**：连接状态、presence、draft、following 各自为政；多条 WS。
8. **UI 状态映射重复**：`SubagentStatus` → `ThreadStatus`、tool status → badge variant、thread status → label/variant 分散在多个组件。

---

## 4. 证据索引

| 状态机 | 主要证据 |
| --- | --- |
| SM-01 | `model.rs:44`、`threads.rs:189/263-272/637`、`agents.rs:119/268/436/501/690/722/830/843/855/918/935/1072/1223`、`ws.rs:204`、`reducer.rs:91/189/218/224/260/269/274` |
| SM-02 | `agents.rs:40-56/289/428-434/466/562/795-803/834-856/966-969/1008-1016` |
| SM-03 | `delivery.rs:35-68`、`api.rs:124`、`model.rs:79`、`agents.rs:281-370`、`events.rs:339-353` |
| SM-04 | `model.rs:111`、`approvals.rs:94/149/227-247/940`、`reducer.rs:188/208/210/216` |
| SM-05 | `model.rs:157`、`agents.rs:58-64/708`、`approvals.rs:403-411/602-606`、`reducer.rs:240-246` |
| SM-06 | `model.rs:57`、`agents.rs:528/727-742/1213`、`threads.rs:877`、`acp.rs:1346-1363`、`SidebarTree.tsx:470-477`、`ThreadPage.tsx:70-77` |
| SM-07 | `model.rs:89`、`agents.rs:963-1031/1056`、`queue.rs:52-56/74/114`、`routes_agent.rs:177/219/249-253` |
| SM-08 | `agents.rs:33/117-122/220/1077`、`threads.rs:180/419-420` |
| SM-09 | `model.rs:239/258`、`reducer.rs:155-162`、`acp.rs:1172/1249`、`ThreadTimeline.tsx:532` |
| SM-10 | `presence.rs`、`ws.rs:20-180`、`live.ts:12/101/228`、`presence.ts`、`hooks.ts:26`、`Composer.tsx:93-126` |
| SM-11 | `drafts.rs:130-137`、`Composer.tsx:120-176`、`docs/06` §10 |
| SM-12 | `approvals.rs:698/774-790`、`routes_agent.rs:261-277`、`threads.rs:503-539`、`ws.rs:32` |
| SM-13 | `diff.rs:130/292-325`、`ChangesPanel.tsx:118-198`、`StatusBar.tsx:101-113` |
| SM-14 | `model.rs:168`、`comments.rs:138-141/166/231/278`、`CommentsPanel.tsx:82/172/203-215` |
| SM-15 | `auth.rs:168-183`、`auth_status`、`me` |
| SM-16 | `sharing.rs:271/284/477` |
| SM-17 | `projects.rs:44`、`threads.rs:546` |
| SM-18 | `routes_workspace.rs` thread_bundle、`ws.rs:184-213`、`queries.ts`、`WorkspaceLayout.tsx:134-145`、`ProjectPage.tsx:182-190` |
| SM-19 | `agents.rs` Capabilities、`routes_agent.rs` agent-config、`settings.rs` |
| SM-20 | `settings.rs`、`live.ts` browserNotify、`approvals.rs` webhook |

---

## 5. 差距清单（输入给「统一模型决策」）

1. 选定唯一权威源：DB 状态列 vs 事件投影 vs 新状态表。
2. 决定是否保留 reducer 派生；若保留，明确 DB 与投影的一致性策略（谁先写、谁为准、如何对账）。
3. 为每个实体定义合法迁移表与守卫（含终态、并发审批、归档/删除后的冻结规则）。
4. 决定死状态的去留（`Delivery::Dropped`、`ApprovalStatus::Cancelled`、`ElicitationStatus::Expired`、`QueueStatus::Superseded`、`CommentStatus::Draft`）。
5. 决定 UI/agent 共用的投影 API 形态（`ThreadState` / 事件 / 轮询），以及前端状态映射的收口位置。
6. 决定次要机器（presence/draft/inbox/seen/comment）是否纳入同一模型，还是登记为受管机器并明确边界。
7. 迁移计划：DB schema、事件枚举、API 契约、回滚点。
8. 修复已发现的不一致：cancel 后 thread 状态回退、decide 与 reducer 的 running 语义、PromptDone 后 `session.turn` 残留、turn error 后 `RunState` 残留。
