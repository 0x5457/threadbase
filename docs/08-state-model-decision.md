# 08 · 统一显式状态模型决策

## 1. 决策摘要

采用 **模型 B：单一写入入口 + 显式迁移表 + 共享状态投影**。

- **权威持久源**：每个实体仍由自己的 SQLite 表承载状态（`threads.status`、`approvals.status`、`elicitations.status`、`subagents.status`、`queued_inputs.status`、`comments.status`、`seen_files`、`inbox.read_at`、`thread_reads`）。
- **权威迁移逻辑**：新增纯逻辑模块 `crates/core/src/state/`，每个实体一张显式迁移表（transition table）与守卫（guard），不依赖 IO。
- **唯一写入入口**：新增 `crates/server/src/state.rs`（`StateStore`），生命周期状态变更必须先过机器校验，再落库；删除散落的裸 SQL 与重复 parse。`seen_files`/`thread_reads`/`inbox.read_at` 是读模型派生，暂不纳入（见 §10.3）。
- **共享读模型**：新增 `ThreadState` 投影（REST `GET /api/threads/{id}/state` + WS `thread_state` 扩充），UI 与 agent 共用；UI 不再各自映射 status，agent 通过语义摘要读取状态。
- **事件日志**：保持 append-only，作为时间线与审计；不再作为 DB 状态的第二写入源。`core::reducer` 改为只折叠时间线，不再推导 thread status。

备选模型与取舍见 §2；迁移表见 §4；变更与迁移路径见 §5；分阶段计划见 §6；UX/DX/AX 影响见 §8。

---

## 2. 候选模型与取舍

### 模型 A：全事件溯源（event-sourced）

所有状态迁移都追加事件，状态是事件的 fold。

- 优点：单一真相、天然可重放、审计完整、agent 可直接读事件。
- 缺点：需要为 queue/subagent/approval/comment/read/seen 的每次迁移新增事件；事件量与存储上升；首屏需要 fold；对 v1 已验证流程改动过大；presence/draft 本来就不是事件驱动，仍要额外模型。
- 结论：概念最纯，但风险与改动面最大，UX 收益不明显（用户看到仍是同一组状态）。**不采用**。

### 模型 B：单一写入入口 + 显式迁移表 + 共享投影（推荐）

DB 表是权威持久源，`core::state` 是权威迁移逻辑，`ThreadState` 是 UI/agent 的唯一读模型。

- 优点：消除双真相与重复 parse；写入收敛到一处；迁移可测试；UI/agent 共用投影；对事件日志与线协议改动最小；可分阶段落地、逐阶段回滚。
- 缺点：DB 仍是「状态」而事件是「时间线」，需要在文档中明确边界；投影要与 DB 保持同步（当前为最终一致，见 §10.3）。
- 结论：**采用**。在 v1 稳定址上同时改善 UX/DX/AX，风险最低。

### 模型 C：混合（核心簇事件溯源，次要机器 DB）

- 优点：核心簇获得事件溯源好处。
- 缺点：两套模式并存，DX 更差；边界模糊；维护成本最高。
- 结论：**不采用**。

---

## 3. 目标架构

```
core::state（纯逻辑，无 IO）
  ThreadMachine        // thread status + turn lifecycle
  ApprovalMachine
  ElicitationMachine
  SubagentMachine
  QueueMachine
  DeliveryMachine
  CommentMachine
  ReadMachine          // thread_reads / inbox / seen 的派生规则
  TransitionError      // IllegalTransition / GuardFailed / InvalidState

server::state::StateStore（唯一写入入口）
  transition(thread_id, entity, action) -> Result<Next, TransitionError>
  落库 + 发布 WS thread_state + （必要时）追加时间线事件

ThreadState 投影（REST + WS）
  UI: useThreadState(status, counts, queue, subagents, ...)
  Agent: ThreadStateSummary（prompt 注入）
```

### 3.1 权威源划分

| 实体 | 权威持久源 | 权威迁移逻辑 | 读模型 |
| --- | --- | --- | --- |
| Thread / Turn | `threads.status` + `threads.last_turn_sha` | `ThreadMachine` | `ThreadState.status` / `current_turn` |
| Approval | `approvals.status` | `ApprovalMachine` | `ThreadState.pending_approvals` |
| Elicitation | `elicitations.status` | `ElicitationMachine` | `ThreadState.pending_elicitations` |
| Subagent | `subagents.status` | `SubagentMachine` | `ThreadState.subagents` |
| Queue | `queued_inputs.status` | `QueueMachine` | `ThreadState.queue` |
| Delivery | 无持久状态（事件字段） | `DeliveryMachine` | `SendMessageResponse.delivery` |
| Comment | `comments.status` | `CommentMachine` | `ThreadState.comments` |
| Thread read | `thread_reads.last_read_seq` | `ReadMachine` | `ThreadState.unread` |
| Inbox read | `inbox.read_at` | `ReadMachine` | `InboxItemDto.read` |
| Changes seen | `seen_files` | `ReadMachine` | `ThreadState.changes.seen` |
| Presence | 内存 `PresenceHub` | `PresenceMachine`（规则） | presence WS |
| Draft | Y.Doc + `draft_updates` | `DraftMachine`（规则） | Composer 本地状态 |
| Auth / Invite | `auth_sessions` / `invites` | 请求时校验 | `AuthStatusDto` / `InviteDto` |
| Project / diverged | 每次请求计算 | `ReadMachine` | `ThreadState.diverged` |

### 3.2 单一写入入口

- `StateStore` 提供 `transition`：先调用对应机器的 `transition(current, action, ctx)`，成功后在一个 SQLite 事务内落库，并发布 WS `thread_state`。
- 所有旧写法（`Pump::set_status`、`Inner::set_thread_status`、`threads.rs` 裸 SQL、`approvals.rs` 裸 SQL、`queue.rs` 裸 SQL、`comments.rs` 裸 SQL、`diff.rs` seen 写入）改为调用 `StateStore`。
- 机器返回 `Next` 与副作用命令（如「取消 pending elicitations」「supersede queue」「发事件」），由 `StateStore` 执行；机器本身不碰 IO。

### 3.3 共享投影 API

- REST：`GET /api/threads/{thread_id}/state` → `ThreadState`。
- WS：扩充现有 `ServerWsMessage::ThreadState`，字段与 REST 投影一致；每次状态迁移后推送。
- UI：`useThreadState(threadId)` 订阅 WS + 初始 REST；`ThreadStatusBadge`、计数、队列、subagent 状态全部从投影读取。
- Agent：`ThreadStateSummary`（文本）在 prompt 起始注入，包含 status、pending approvals/elicitations、queue 长度、subagent 状态；结构化数据保留在同一投影中，后续可加 ACP 工具。

### 3.4 事件日志边界

- 事件保持 append-only；时间线事件（消息、工具、plan、评论、成员、队列更新）不变。
- 生命周期事件（`permission_requested/resolved`、`elicitation_requested/resolved`、`subagent_updated`、`turn_*`、`thread_updated`）保留，用于时间线展示与审计。
- **不再**用 reducer 从事件推导 thread status；reducer 只负责消息/工具/plan/评论等时间线折叠。
- 状态迁移与事件追加在当前实现中是两个独立事务，通过投影修订与顺序广播保证最终一致（见 §10.3）；不假设同事务。

---

## 4. 各实体目标迁移表与守卫

### 4.1 ThreadMachine（thread + turn）

状态：`preparing | idle | running | requires_action | interrupted | error | archived`。

| 当前 | 动作 | 目标 | 守卫 |
| --- | --- | --- | --- |
| — | create | preparing | 初始（创建态） |
| 任意非归档 | restart | preparing | archived 拒绝 |
| preparing | start_ok | idle | — |
| preparing | start_fail | error | — |
| idle/running/requires_action/error/interrupted | prompt | running | preparing/archived 拒绝 |
| idle/running/requires_action/error/interrupted | approval_pending | requires_action | preparing/archived 拒绝 |
| idle/running/requires_action/error/interrupted | elicitation_pending | requires_action | preparing/archived 拒绝 |
| requires_action | approval_resolved | running/requires_action | 按剩余 pending 数与 turn_active |
| requires_action | elicitation_resolved | running/requires_action | 同上 |
| running/requires_action/idle | turn_ended | idle | — |
| 任意非 preparing/archived | turn_error | error | — |
| 任意非 preparing/archived | process_exited | interrupted | — |
| 任意非 archived | agent_failed | error | — |
| running/requires_action/idle | cancel | idle | 同时取消 pending approvals/elicitations、supersede queue |
| 任意非归档 | archive | archived | 需 manage 角色 |
| archived | unarchive | idle | 需 manage 角色 |

守卫：
- `archived` 为冻结态，除 `unarchive` 外拒绝一切迁移。
- `requires_action` 可由任意非 preparing/archived 状态进入；回退目标由 pending 集合与 turn_active 决定。
- `error`/`interrupted` 可由 `restart`/`prompt` 恢复。
- 并发审批时，`requires_action → running` 仅当 pending 数为 0。

### 4.2 ApprovalMachine

状态：`pending | allowed | denied | cancelled`。

| 当前 | 动作 | 目标 | 守卫 |
| --- | --- | --- | --- |
| — | create | pending | — |
| pending | allow | allowed | write 角色；scope 决定规则 |
| pending | deny | denied | write 角色 |
| pending | cancel | cancelled | turn 取消/线程删除 |

守卫：只有 `pending` 可迁移；`allowed/denied/cancelled` 为终态；迁移后重算 thread status。

### 4.3 ElicitationMachine

状态：`pending | accepted | declined | cancelled | expired`。

| 当前 | 动作 | 目标 | 守卫 |
| --- | --- | --- | --- |
| — | create | pending | — |
| pending | accept | accepted | write 角色 |
| pending | decline | declined | write 角色 |
| pending | cancel | cancelled | turn 取消 |
| pending | expire | expired | 超时（若实现） |

守卫：只有 `pending` 可迁移；`expired` 是否启用见 §5.3。

### 4.4 SubagentMachine

状态：`idle | running | requires_action | error | cancelled`。

| 当前 | 动作 | 目标 | 守卫 |
| --- | --- | --- | --- |
| — | create | idle/running | adapter 上报 |
| idle/running | run | running | — |
| running | require_action | requires_action | — |
| requires_action | resolve | running | — |
| idle/running/requires_action | fail | error | — |
| idle/running/requires_action | cancel | cancelled | — |

守卫：`error/cancelled` 为终态；服务端只接受已知状态；`can_cancel` 与可取消状态一致（`cancel_subagent` 用 `is_terminal || !can_cancel` 拒绝终态取消）。

### 4.5 QueueMachine

状态：`queued | sending | sent | superseded | dropped`。

| 当前 | 动作 | 目标 | 守卫 |
| --- | --- | --- | --- |
| — | enqueue | queued | position 单调 |
| queued | dispatch | sending | 线程空闲；同线程仅一个 sending |
| sending | prompt_ok | sent | — |
| sending | prompt_fail | dropped | — |
| queued | supersede | superseded | turn 取消 |
| queued | delete | dropped | 用户删除 |
| queued/superseded | retry | queued | Composer continue |

守卫：`sent/dropped` 为终态；`superseded` 可经 retry 回到 `queued`（不是终态）；`superseded` 作为 status 写入（布尔列保留为冗余索引）。

### 4.6 DeliveryMachine

`DeliveryMode → SendAction → Delivery` 的纯映射，保留 `delivery.rs`，补 `Dropped` 的实际语义：steering 失败或队列丢弃时返回 `Dropped`。

### 4.7 CommentMachine

状态：`pending | delivered | resolved`（`draft` 移除或仅在显式草稿流程中启用）。

| 当前 | 动作 | 目标 | 守卫 |
| --- | --- | --- | --- |
| — | create(send=false) | pending | — |
| — | create(send=true) | delivered | — |
| pending | deliver | delivered | — |
| pending/delivered | resolve | resolved | 作者或 owner |

### 4.8 ReadMachine（thread read / inbox / seen）

- thread unread = `max(events.seq) > thread_reads.last_read_seq`。
- inbox unread = `read_at IS NULL`。
- seen = `seen_files.content_hash` 与当前文件 hash 一致。
- 规则集中在一处，供 `threads::dto`、`ws`、`diff::stats`、`InboxPage` 共用。

### 4.9 PresenceMachine / DraftMachine

- presence：`disconnected → connecting → connected → disconnected`；`typing/hidden` 为属性；following 为客户端派生。
- draft：块 `editing → sent(tombstone) | deleted`；provider `connecting → connected → synced`；发送竞争由服务端 `send_guard`（peek → send → take）串行保证。
- 这两台机器以规则文档 + 测试形式落地，不引入新的运行时状态存储。

---

## 5. 事件 / API / DB 变更清单与迁移路径

### 5.1 API

- 新增：`GET /api/threads/{thread_id}/state` → `ThreadState`。
- 扩充：WS `ThreadState` 消息字段与 REST 投影一致。
- 保留：现有 bundle / queue / approvals / comments 端点，避免破坏前端与测试；UI 逐步切换到投影。
- 废弃（标记，不立即删）：`ThreadDto.status` 的客户端派生映射（`SidebarTree` 的 `SubagentStatus → ThreadStatus`、`ThreadPage.subagentStatus`）。

### 5.2 DB

- 新增 migration：
  - `queued_inputs`：增加 `superseded_at`，把 `superseded` 布尔纳入 status 迁移（或保留布尔但统一读写）。
  - `approvals` / `elicitations` / `comments`：补 `CHECK` 约束限定合法 status。
  - `threads`：补 `CHECK` 约束限定合法 status。
- 不删列、不重命名；additive only，便于回滚。
- 为 `thread_reads`、`seen_files`、`inbox` 的读模型补索引（如需要）。

### 5.3 事件

- 事件枚举：新增 `permission_cancelled`（turn 取消时取消 pending approval）；其余保持。
- 一致性：状态落库后广播 `ThreadState` 投影；事件作为时间线/审计单独追加（状态是权威，投影由状态推导，避免双写不一致）。
- 死状态处理（已落地）：
  - `Delivery::Dropped`：已启用（steering 失败时返回 Dropped）。
  - `ApprovalStatus::Cancelled`：已启用（turn 取消时取消 pending approval，并发 `permission_cancelled`）。
  - `ElicitationStatus::Expired`：已移除（v1 无超时）。
  - `QueueStatus::Superseded`：已启用，写入 status。
  - `CommentStatus::Draft`：已移除（v1 无草稿评论）。
  - `agent_bindings.status`：保留为诊断字段，非权威（`INITIAL_BINDING_STATUS` 写入）。

### 5.4 迁移路径

1. Phase 1：`core::state` 机器 + 测试（纯逻辑，无行为变化）。
2. Phase 2：`server::state::StateStore` 单一写入；替换裸 SQL；修复不一致；DB migration。
3. Phase 3：`ThreadState` REST + WS 投影；后端推送。
4. Phase 4：UI 切换到投影；agent 摘要注入。
5. Phase 5：次要机器（presence/draft/inbox/seen/comment）纳入规则与测试。
6. Phase 6：全量验证、文档、截图。

---

## 6. 分阶段计划与验收标准

| 阶段 | 内容 | 验收 |
| --- | --- | --- |
| P0 | 审计 + 决策 | `docs/07`、`docs/08` 存在；覆盖全部实体 |
| P1 | `core::state` 机器 | 每个实体有迁移表；合法/非法/守卫/幂等测试通过；core 无 IO |
| P2 | 后端单一写入 | rg 证明无重复 parse/裸写；不一致修复；nextest 0 失败 |
| P3 | 投影 API | REST/WS 投影可用；UI 能订阅；测试覆盖 |
| P4 | UI + agent | 前端无 status 重映射；agent 摘要可读；playwright 0 失败 |
| P5 | 次要机器 | presence/draft/inbox/seen/comment 规则与测试落地 |
| P6 | 全量验证 | 质量门全绿；端到端闭环；docs/06 更新 |

每阶段通过后进入下一阶段；未通过则停留在本阶段修复。

---

## 7. 回滚点

- 每个 Phase 一个提交（或一组提交），保证可独立回滚。
- DB migration 提供 `down` 脚本（`0001_init.down.sql` / `0002_approval_input.down.sql` / `0003_state_indexes.down.sql`，sqlx `Migrator::undo` 可回滚）；additive 变更可安全回滚。
- API 旧端点保留，UI 切换是渐进的；必要时可回退到 bundle 派生。
- 事件枚举只增不删（除死状态清理），保证重放兼容。

---

## 8. UX / DX / AX 影响

### UX

- 状态一致：DB、投影、UI、agent 读到同一状态；不再出现「DB running / 投影 requires_action」的分裂。
- 取消/失败可回退：cancel 后 thread 立即回到 idle；error/interrupted 有明确恢复路径。
- 冻结态清晰：archived 不再被事件覆盖；waiting 计数与实际 pending 一致。
- 状态矩阵（空/加载/错误/等待/断线/准备中/分叉/只读）由投影统一驱动，减少页面各自实现。

### DX

- 一个实体一张迁移表、一个写入入口、一个 parse；rg 可证重复消除。
- 迁移有测试：合法/非法/守卫/幂等。
- 新增状态或动作只改 `core::state` 一处，并自动获得守卫与测试。
- 投影 API 明确 UI/agent 的边界，减少「这个状态谁负责」的歧义。

### AX

- agent 在 prompt 起始获得 `ThreadStateSummary`：status、pending approvals/elicitations、queue 长度、subagent 状态。
- 子 agent 状态映射统一（ACP → SubagentMachine → 投影），不再由前端二次翻译。
- 语义锚点保持不变；agent 可继续通过锚点理解上下文。
- 能力缺失时文档化退化路径（如 adapter 不上报 subagent 事件时，投影显示 idle + 说明）。

---

## 9. 风险

- 投影与 DB 同步：当前为最终一致 + 投影修订；不假设同事务（见 §10.3）。
- 事件量：不新增事件类型，避免膨胀。
- 迁移兼容：旧 DB 的 status 值可能包含死状态；migration 需要清洗或映射。
- 前端切换：投影字段要覆盖现有 UI 全部状态，否则会出现回归；用 playwright 状态矩阵兜底。

---

## 10. 落地记录（P1–P2 与 AX）

### 10.1 已实现

- `crates/core/src/state.rs`：ThreadMachine、ApprovalMachine、ElicitationMachine、SubagentMachine、QueueMachine、AgentSessionMachine、DeliveryMachine、CommentMachine、ReadRules、PresenceMachine、DraftMachine，以及全部状态字符串映射；测试覆盖合法/非法/守卫/幂等。
- `crates/server/src/state.rs`：`StateStore` 是 thread / approval / elicitation / queue / comment / subagent / agent_binding 生命周期状态列的唯一写入入口——这些状态迁移都经对应机器校验后在此落库，并广播 `ThreadState` 投影。`seen_files`/`thread_reads`/`inbox.read_at` 是读模型派生，仍由各自模块写入（见 §10.3）。subagent 的创建与迁移由 `create_subagent` 统一处理；queue retry 经 `QueueMachine::Retry`。
- 创建态写入（thread/approval/elicitation/comment/queue 的 INSERT）也经机器的 `Create`/`Enqueue` 动作校验初始状态，消除了死代码。
- 死状态已落地：`ElicitationStatus::Expired`、`CommentStatus::Draft` 已移除；`ApprovalStatus::Cancelled`（turn 取消时取消 pending approval）、`Delivery::Dropped`（steering 失败）已启用；新增 `permission_cancelled` 事件。
- `ThreadState` 投影（`crates/shared/src/api.rs`）：`thread_id / status / current_turn / pending_approvals / pending_elicitations / queued_inputs / active_subagents / diverged`。REST `GET /api/threads/{id}/state`、WS `thread_state`、agent 摘要三者共用同一投影。
- `AppState` 新增 `state_tx` / `state_store`；`ws.rs` 订阅状态广播。
- `threads.rs`、`ws.rs`、`agents.rs`、`approvals.rs`、`queue.rs`、`routes_agent.rs`、`comments.rs`、`events.rs` 的重复 parse/status_str 已删除，统一走 `core::state`。
- `core::reducer` 不再推导 thread status（`TimelineState` 无 status 字段），消除 DB 与事件投影的双真相。
- `0003_state_indexes.sql`：为 pending/queue/read 查询补索引；`0001/0002/0003` 均提供 `.down.sql`（已用 sqlite3 验证 up→down）。
- 守卫落地：`SubagentMachine::is_terminal`/`can_cancel` 用于 `cancel_subagent`；`QueueMachine::is_terminal` 用于 `retry_queue_item`。
- 修复的不一致：cancel 后 thread 回到 idle；decide/elicitation 后按剩余 pending 决定 running；archive 同时写 `archived_at`；`auto_allow` 对并发决定幂等；Pump 事件循环不再因单次状态错误而终止。

### 10.2 AX：agent 语义状态摘要

- 服务端在 prompt 起始从 `ThreadState` 投影生成 `[Thread state] status=…; current_turn=…; pending_approvals=…; pending_elicitations=…; queued_inputs=…; active_subagents=…`，作为附加文本块注入。
- 仅当 pending/queued/active_subagents 非零时注入，避免正常 prompt 噪音。
- 摘要与 UI 共用同一投影，**不依赖 adapter 能力**；adapter 不上报 subagent/elicitation 时，摘要仍准确（计数来自服务端表）。这是能力缺失时的降级路径：不伪造能力，只报告服务端事实。
- 测试：`state::tests::summary_reports_pending_state`（投影内容）、`agent_flow::agent_prompt_includes_state_summary_when_pending`（注入到真实 prompt）。

### 10.3 与决策的偏差

- DB `CHECK` 约束未加：SQLite 需要重建表，风险高于收益；改由 `core::state` 机器在应用层强制，并补索引。
- `agent_bindings.status` 保留为诊断字段，但文档明确它不是权威状态。
- `QueueStatus::Superseded` 现在真正写入 status；`superseded` 布尔保留为冗余。
- 状态落库与事件 append 是两个独立事务，不是同事务；投影通过 revision/顺序广播保证最终一致。
- `ThreadState.current_turn` 已落到 `threads.current_turn_id`（migration 0005，见 docs/06 §13.5），不再从事件派生；`AgentManager::current_turn` 同步读列。
- `seen_files`/`thread_reads`/`inbox.read_at` 仍由 `diff.rs`/`routes_agent.rs`/`approvals.rs` 直接写入，未纳入 `StateStore`；它们是读模型派生，后续可收敛。

---

## 11. 次要机器受管

### 11.1 权威源与规则

| 机器 | 权威源 | 规则/入口 | 测试 |
| --- | --- | --- | --- |
| Comment | `comments.status` | `CommentMachine`（create/deliver/resolve/submit_draft），写入经 `comment_status_str` | core `comment_*`、`comment_transition_to` |
| Thread read | `thread_reads.last_read_seq` | `ReadRules::thread_unread`，已接入 `threads::dto` | core `read_rules` |
| Inbox read | `inbox.read_at` | `ReadRules::inbox_unread`，已接入 `approvals::list_inbox` | core `read_rules` |
| Changes seen | `seen_files.content_hash` | `ReadRules::seen`，已接入 `diff::list` | core `read_rules` |
| Presence | 内存 `PresenceHub` | `PresenceMachine`（连接迁移、hidden 不影响在线、广播抑制、30s follow 过期） | core `presence_rules` |
| Draft | Y.Doc + `draft_updates` | `DraftMachine`（provider connecting→connected→synced；发送需 synced；sentAt 墓碑） | core `draft_rules` |
| Auth / Invite | `auth_sessions` / `invites` | 请求时校验；无持久状态机，不列入迁移表 | 现有 auth/sharing 测试 |
| Project / diverged | 每次请求计算 | `ReadRules` 同级的派生规则（path_available / base_sha 对比） | workspace 测试 |

### 11.2 边界说明

- Presence 与 draft 是**规则集中、存储分散**：服务端口层状态在内存 PresenceHub，草稿在 Y.Doc；两者不引入新的持久状态表，避免过度建模。
- 前端连接状态仍按逻辑通道分离（events / presence / drafts），但都遵循同一 `PresenceMachine` 规则；未做单一 WS 合并，属于后续可选优化。
- Auth/Invite 不涉及高频状态迁移，保持请求时校验，不强行套迁移表。
