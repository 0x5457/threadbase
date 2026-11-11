# 09 · 审计发现与修正清单

- 日期：2026-11-10
- 输入：产品研究（`docs/research` 子 Agent 报告）、架构研究（子 Agent 报告）、`docs/01–08` 与 `README.md`、`crates/**`、`web/src/**` 交叉验证
- 用途：task-1 产出，供 task-2（文档修正）、task-3（后端修复）、task-4（前端修复）执行
- 口径：**文档修正** = 只改文档；**实现修复** = 改代码并补测试；**裁决** = 在 `docs/06` 记录不修的理由

结论摘要：产品方向与架构方向均成立，无根本性错误。问题集中在三类：文档与实现脱节、少数产品级矛盾未拍板、草稿/状态层的高风险工程缺口。以下按 P0/P1/P2 排序。

---

## P0 · 产品前提、安全语义、数据正确性

### P0-1 agent 凭据归属自相矛盾（文档修正）

- 出处：`docs/01-requirements.md:18`、`:35` vs `docs/01-requirements.md:287`、`docs/02-open-decisions.md:16`、`README.md`
- 证据：01 §1.3 写「由用户自己安装、自己登录、自己付费」，§3 写「各自有订阅或 API 额度」；但 01 §10 与 02 已定 1 写「agent 凭据属于机器，不属于用户，所有参与者共用这台机器上的 agent 登录」。README 与代码均无按用户隔离凭据。
- 期望：明确「所有 turn 都在机器主人的 agent 账号下执行，成本与审计记在该账号」。
- 当前：同一文档内两种前提并存，误导成本归属与审计预期。
- 修复方案：采用方案 A（承认共享），删除 01 §1.3/§3 的「自己付费/各自额度」表述，改为机器账号模型；UI 固定展示当前 agent 身份（02 已定 1 已要求）。方案 B（按人启动会话）是架构级改动，v1 不做。

### P0-2 URL 模式提问的验证口径与代码事实不符（文档修正 + 裁决）

- 出处：`docs/06-implementation-status.md:82`、`:110` vs `crates/agent/src/acp.rs:362-363`、`:969-978`
- 证据：06 声称「表单/URL 两种模式已验证」「URL 模式提问 已验证」；但 `client_capabilities` 声明 `elicitation_url: false`，`client_capabilities()` 只声明 `.elicitation(...form(...))`。agent 不会发起 URL 模式提问。
- 期望：验证声明与代码事实一致。
- 当前：06 的「已验证」只可能覆盖收到 URL 模式时的渲染路径，不代表产品可用。
- 修复方案：二选一并记录裁决——(a) 启用 `elicitation.url` 能力并补真实 agent 冒烟；(b) 把 06 改为「渲染路径已实现，协议能力未启用，待 M4」。在拍板前不得声称已验证。

### P0-3 默认「停止并发送」具破坏性且归因错误（产品决策 + 实现 + 文档）

- 出处：`docs/01-requirements.md:215`、`docs/04-subagents-and-questions.md:172`、`docs/05-ui-spec.md:175` vs `crates/core/src/delivery.rs:33-67`
- 证据：`resolve_delivery(DeliveryMode::Auto, turn_running=true, steering_supported=false)` 返回 `StopAndSend`；ACP v1 要求 cancel 时响应全部 pending 权限；08 §4.1 把 cancel 与「取消 pending 审批/提问、supersede 队列」绑定。04/05 称其为「Delta 的行为」，但 Delta 文档无 steering 前提，归因错误。
- 期望：默认不打断正在运行的 turn；如需打断必须显式确认并提示会取消当前 turn 与未处理审批/提问。
- 当前：不支持 steering 时，用户随手发消息即静默取消当前 turn 与未处理审批。
- 修复方案：不支持 steering 的 agent 默认改为「排队」；若保留「停止并发送」，UI 必须二次确认并说明后果；修正 04/05 的 Delta 归因。

### P0-4 子会话「只读」描述自相矛盾（文档修正）

- 出处：`docs/01-requirements.md:213` vs `docs/04-subagents-and-questions.md:95`
- 证据：01 §7 写「子会话只读，不能向它发 prompt」；04 §3.3 写「子会话可以写文件、跑命令，不受『只读』限制；受限的只是用户不能给它派活」。ACP v1 子 agent RFD 禁止客户端向子会话发 prompt，但不限制子会话工作区写权限。
- 期望：统一为「子会话在工作区上可写，但用户不能向它派活」。
- 当前：01 的「只读」会被误解为工作区只读。
- 修复方案：按 04 §3.3 修正 01 §7 与 01 §6 F12。

### P0-5 草稿发送非原子，失败丢草稿、第二发送者语义不符（实现修复）

- 出处：`crates/server/src/routes_agent.rs:43-62`、`crates/server/src/drafts.rs:127-148`、`crates/draft/src/lib.rs:217-253` vs `docs/03-tech-stack.md:181-184`
- 证据：`send_message` 先 `take_blocks()` 消费 Y.Doc（写 `sentAt` 墓碑）再调 `agents.send()`；发送失败时草稿已丢失。第二个发送者得到 422「message cannot be empty」，而非文档承诺的 `already_sent`。`take_trailing()` 取所有非空块，块级作者信息在拼接时丢失。
- 期望：发送可恢复；第二发送者得到可区分的空草稿/已发送语义；保留作者列表。
- 当前：agent 未启动/崩溃/发送失败时用户输入丢失；多端同时发送第二人看到报错且文本消失。
- 修复方案：先创建消息/预占再消费草稿，或发送失败时回滚 Y.Doc（清 `sentAt`、恢复文本）；空结果改为可区分响应；明确 `take_trailing` 语义并保留 `authors[]`。

### P0-6 入站 CRDT 更新无资源约束（实现修复）

- 出处：`crates/server/src/drafts.rs:76-97`、`:201-240` vs `crates/draft/src/lib.rs:15-16`、`docs/03-tech-stack.md:190-192`
- 证据：大小限制只在服务端主动写路径生效；`DraftRoom::apply` 直接 `handle_update` 并 `persist_update`，不校验 update 长度、块数、thread 总字节。`sentAt` 墓碑永不回收，`DraftHub.rooms` 只增不减。
- 期望：入站更新有大小/块数/总量上限；墓碑与房间有回收机制。
- 当前：异常客户端可致 SQLite 与内存无限增长；长期运行快照膨胀。
- 修复方案：在 `apply`/`persist_update` 校验 update 长度、单块文本、thread 总字节、块数；为 WebSocket 设置 `max_message_size`/`max_frame_size`；增加墓碑压缩与房间空闲淘汰。

### P0-7 Seen 状态与 Changes 基线耦合（实现修复 + 需求补充）

- 出处：`crates/server/src/diff.rs:311-324`、`:457`、`docs/07-state-machines.md` SM-13 vs `docs/05-ui-spec.md` §7
- 证据：`seen_files` 按 `(thread, user, path, content_hash)` 存储，不含 baseline。05 §7 允许自由切换四档基线。
- 期望：Seen 按 (用户, 文件, 基线种类, 内容 hash) 记录；切换基线不继承 Seen。
- 当前：在 Last Turn 下 Seen 切到 Last Commit 会错误沿用；反之错误丢失。
- 修复方案：`seen_files` 增加 baseline 维度（或等价 key），同步读写与 `ReadRules::seen`；在 01 §6 F7 补充该约束。

---

## P1 · 文档对齐与工程加固

### P1-1 审批 scope 语义与协议粒度（文档修正 + UI 说明）

- 出处：`docs/01-requirements.md:102`、`:213`、`:307`、`docs/04-subagents-and-questions.md:115` vs `crates/agent/src/acp.rs:1447-1465`、`crates/server/src/approvals.rs:125-129`
- 证据：ACP `PermissionOptionKind` 只有 `allow_once/allow_always/reject_once/reject_always`，`allow_always` 无 session/thread 粒度。代码把 `AllowSession`/`AllowThread` 都映射到 `allow_always`，产品层用 `approval_rules` 约束自动放行范围。01 §11 的「按 project 限定」与代码的匹配条件不是一回事；02 已定 6 漏了 session 档。
- 期望：文档统一为三档（本次/本会话/本 thread），并说明产品层规则与协议 `allow_always` 的边界。
- 当前：三处定义不一致，安全语义未写明。
- 修复方案：01/02/04 统一 scope 定义；UI 说明「一律允许」由产品规则约束，协议层记忆范围由 agent 决定。

### P1-2 分享档位三档 vs 两档（文档修正）

- 出处：`docs/01-requirements.md:102` vs `docs/02-open-decisions.md` 已定 15、`crates/shared/src/model.rs:176-178`
- 证据：01 §5 写「仅邀请、团队成员、持链接可看」三档；代码 `ShareLevel::{InviteOnly, Workspace}` 两档。
- 期望：文档与实现一致为两档。
- 当前：01 未同步。
- 修复方案：01 §6 F5 改为两档，删除「持链接可看」。

### P1-3 状态与事件的事务一致性（实现修复或裁决）

- 出处：`crates/server/src/state.rs:90-113` vs `crates/server/src/events.rs:62-124`、`docs/08-state-model-decision.md:103`、`:331`
- 证据：状态落库与事件 append 是两个独立事务；08 声称「同一事务」。`docs/08:331` 声称 `StateStore` 是所有生命周期状态列的唯一写入入口，但 `seen_files`（`diff.rs:311,324`）、`thread_reads`（`routes_agent.rs:250`）、`inbox.read_at`（`approvals.rs:732,739`）仍绕过。
- 期望：要么同事务，要么文档改为「最终一致 + 可观测告警」，并登记裸写清单。
- 当前：文档过强，实现不满足；并发/崩溃下可能出现状态与时间线不一致。
- 修复方案：优先给 `ThreadState` 加单调 revision 并串行化广播；把 `seen/read/inbox` 纳入 `StateStore` 或明确豁免；修正 08 表述。

### P1-4 ThreadState 无版本号，投影广播可能乱序（实现修复）

- 出处：`crates/shared/src/api.rs:307-316`、`crates/server/src/state.rs:115-179`
- 证据：`publish_thread_state` 是「先 `await projection()` 再 `send`」，两个并发迁移的投影计算与发送顺序可交错；`ThreadState` 无 `revision`。
- 期望：消费方丢弃旧版本，或广播串行化。
- 当前：UI 可能回退到旧状态。
- 修复方案：加 `revision`（`threads.updated_at` 或计数器），前端/投影消费方按版本丢弃乱序；或每 thread 串行化广播。

### P1-5 current_turn 仍从事件派生（实现修复或裁决）

- 出处：`crates/server/src/state.rs:115-122`、`crates/server/src/agents.rs:639-653` vs `docs/08-state-model-decision.md:11`
- 证据：投影的 `current_turn` 查最新 `turn_started` 事件；08 声称事件不再是 DB 状态的第二写入源，但 turn 身份仍是事件派生。
- 期望：turn 身份由 DB 承载，事件只保留时间线与内容。
- 当前：投影依赖事件，与 08 的权威源划分不符。
- 修复方案：把 `current_turn` 落到 `threads`（如 `current_turn_id`），投影只读 DB；或明确记录为可接受的派生并修正 08 表述。

### P1-6 docs/03 版本、依赖与实现描述大面积漂移（文档修正）

- 出处：`docs/03-tech-stack.md` 全文 vs `docs/06-implementation-status.md` §1、`Cargo.toml`、`web/package.json`、代码
- 证据：ACP 5.0→5.1.0、sqlx 0.9→0.8.6、yrs 0.28→0.17.4、React Router 8→7.18.5；列出的 `tower-sessions`、`tower-governor`、`axum-extra`、`insta`、`proptest` 均未使用；ACP 接入方式（`Client` trait vs 5.1 回调模型）、Harness trait 面、进程并发上限/空闲回收、草稿发送/持久化、实时层「三通道共用一条 TCP」「1MB 缓冲」「两次未响应断开」「presence 按 (thread,user)」等均与实现不符。
- 期望：以 06 §1 版本审计为准修正 docs/03。
- 当前：docs/03 是选型阶段的文档，未随实现更新。
- 修复方案：重写 docs/03 的过时段落；删除未使用依赖；修正实时层、进程治理、草稿发送描述。

### P1-7 docs/07 未标注已被 docs/08 取代（文档修正）

- 出处：`docs/07-state-machines.md` 文首
- 证据：07 是决策前审计，08 §10 已记录 P1–P2 落地与修复；07 文首仍写「作为下一阶段决策的输入」。
- 期望：读者知道 07 是历史输入，不是现状。
- 当前：可能把已修复问题当作现状。
- 修复方案：文首加「已被 docs/08 取代，仅作历史输入」横幅。

### P1-8 docs/05 视觉基准、终端规格、移动端范围过时（文档修正）

- 出处：`docs/05-ui-spec.md` 全文、`:92-93`、`:182-185`、`:330-379` vs `docs/06-implementation-status.md` §9/§11、`web/src`
- 证据：05 以 Delta v0.24.0 深色为照抄基准；实现已转向 Linear/Apple Notes + shadcn + 浅色主题（06 §9/§11）。05 §5.4 仍规格化 `!` 内联终端与 Terminals picker，但 v1 明确不做 PTY（06 §2/§8）。05 §4.1 设计移动端 tab 条，01 §4 说不做移动端专用客户端。05 §11 的 `⌘⇧M` 与实现不一致。
- 期望：保留交互语义，视觉基准改为实现方向；删除未实现的终端规格；澄清移动端范围。
- 当前：05 与实现、与 06 冲突。
- 修复方案：新增「与 Delta 的视觉偏离」一节；把终端规格移入「不做承诺的项」；把移动端 tab 条标注为响应式适配；对齐键位表。

### P1-9 热路径性能陷阱（实现修复或裁决）

- 出处：`crates/server/src/state.rs:140`（`is_diverged` 每次投影调用两次 git 子进程）、`crates/server/src/auth.rs:347-350`（每认证请求写 `last_seen_at`）、`crates/server/src/agents.rs:682`（每 chunk 一条 `MessageDelta`，无合并）
- 证据：`threads::is_diverged` 在 `projection()` 中调用；`last_seen_at` 把读请求变写请求；docs/03:262 声称 200ms/4KB 合并但未实现。
- 期望：投影不被 git 调用拖慢；读请求不放大写锁；流式事件量可控。
- 当前：高并发/长流式下 SQLite 写竞争与事件膨胀。
- 修复方案：`is_diverged` 按需计算或缓存到 `threads`；`last_seen_at` 节流；实现 delta 合并或修正文档。

### P1-10 THREADBASE_PUBLIC_ORIGIN 未文档化（文档修正）

- 出处：`crates/server/src/config.rs:41`、`crates/server/src/middleware.rs:44-48`、`crates/server/src/auth.rs:484-490`、`crates/server/src/main.rs:23` vs `README.md`、`docs/03-tech-stack.md`
- 证据：该变量影响 CSRF 允许的 Origin 与 `Secure` cookie；README 远程访问节未要求设置。若 `tailscale serve` 转发后 Host/Origin 不匹配，所有写操作会 403。
- 期望：文档说明其作用与推荐配置，并提示风险。
- 当前：运维前提缺失。
- 修复方案：README/docs/03 补 `THREADBASE_PUBLIC_ORIGIN` 说明；启动自检在绑回环且未设置时打印远程访问风险提示。

### P1-11 队列「清空」承诺无实现（文档修正或实现）

- 出处：`docs/01-requirements.md:187` vs `crates/server/src/routes_agent.rs`（只有单条 retry/delete）
- 证据：01 写「由任一 editor 决定继续或清空」；代码无批量清空。
- 期望：承诺与实现一致。
- 当前：文档承诺不存在的能力。
- 修复方案：实现批量清空，或把 01 改为「逐条撤回/继续」。

### P1-12 评论状态机与「单独提交」语义未定义（文档修正或实现）

- 出处：`docs/01-requirements.md:174`、`:213`、`docs/04-subagents-and-questions.md:230-237` vs `crates/shared/src/model.rs:168-175`、`docs/07-state-machines.md` SM-14
- 证据：01/04 描述 pending→delivered，代码还有 `resolved`；`CommentStatus::Draft` 曾半死；「单独提交评论链」是否进入 agent 上下文未定义。
- 期望：状态集合与单独提交语义明确。
- 当前：文档与代码状态集合不一致，行为未定义。
- 修复方案：统一状态描述；明确单独提交是否注入 agent；清理死状态或记录裁决。

### P1-13 审批规则管理界面在 UI 规格中缺失（文档修正）

- 出处：`docs/05-ui-spec.md` vs `web/src/components/workspace/ApprovalRulesSection.tsx`、`web/src/routes/SettingsPage.tsx`
- 证据：01 §11 要求「能查看和撤销」，实现已有规则管理；05 无对应规格。
- 期望：UI 规格覆盖规则管理。
- 当前：文档落后于实现。
- 修复方案：05 增加审批规则管理一节。

### P1-14 presence 通道粒度与 300ms 延迟未分解（文档修正）

- 出处：`docs/01-requirements.md` §6 F11、§12 vs `crates/server/src/presence.rs:45-58`
- 证据：代码是 project 级通道、thread 是锚点的一部分；01 未说明。300ms 端到端未分解，与 10 Hz 客户端上限叠加后余量不明。
- 期望：写明 project 级通道与延迟预算。
- 当前：实现与文档粒度表述不清。
- 修复方案：01/02 补通道粒度；把 300ms 拆成典型/最坏。

### P1-15 通知规格缺失（文档修正）

- 出处：`docs/05-ui-spec.md` vs `docs/06-implementation-status.md` §7、`web/src/routes/SettingsPage.tsx`
- 证据：06 §7 有浏览器通知与 Webhook 契约；05 无通知设置规格。
- 期望：UI 规格覆盖通知设置。
- 当前：文档缺失。
- 修复方案：05 增加通知设置节。

### P1-16 adapter 版本锁与文档不一致（文档修正/依赖）

- 出处：`crates/agent/src/profile.rs:21-39` vs `docs/03-tech-stack.md:126-127`、`docs/02-open-decisions.md`
- 证据：codex-acp 锁 2.6.0、claude-agent-acp 锁 0.101.0（最新 0.103.0）；docs/03 仍写 0.16.x 与「Claude 独立可执行文件」。
- 期望：版本锁与文档一致，并发版检查单明确。
- 当前：文档过时。
- 修复方案：更新 docs/03；在 06 记录版本锁与升级检查单。

---

## P2 · 清理与加固

### P2-1 键位与文案对齐

- 出处：`docs/05-ui-spec.md` §11、`docs/06-implementation-status.md` §3.1 F7
- 证据：`⌘⇧M` 规格为「重开已投递评论」，实现是打开 Comments 抽屉；分叉 footer 文案 06 与 05 不一致。
- 修复方案：统一键位与文案，以实现为准或调整实现。

### P2-2 死状态与命名确认

- 出处：`docs/07-state-machines.md` SM-03/04/05/14、`docs/08-state-model-decision.md` §5.3
- 证据：`Delivery::Dropped`、`ApprovalStatus::Cancelled`、`ElicitationStatus::Expired`、`CommentStatus::Draft` 的处理结论分散；08 已移除部分。
- 修复方案：在 01/04 状态描述中反映最终结论；确认无残留死状态。

### P2-3 事件保留策略

- 出处：`crates/server/src/agents.rs:682`、`docs/03-tech-stack.md`
- 证据：每个 `AgentMessageChunk` 一条事件；无归档压缩策略。
- 修复方案：评估事件保留/压缩策略，或明确接受长期增长。

### P2-4 WebSocket 合并、HSTS 与 Cookie 前缀

- 出处：`docs/03-tech-stack.md:297`、`crates/server/src/middleware.rs:62-78`、`crates/server/src/auth.rs:484-490`
- 证据：docs/03 称三通道共用一条 TCP，实际两条 WS；无 HSTS，会话 cookie 无 `__Host-` 前缀。
- 修复方案：修正 docs/03；评估 HSTS/`__Host-`；若不合并在文档明确两条连接的重连语义。

### P2-5 进程治理承诺未实现

- 出处：`docs/03-tech-stack.md:125` vs `crates/server/src/agents.rs`
- 证据：文档称并发上限 3、空闲 15 分钟回收；代码无信号量/回收。
- 修复方案：实现或修正文档；v1 单团队规模下可接受不实现，但需裁决。

### P2-6 写接口限流与文档不一致

- 出处：`docs/03-tech-stack.md:321` vs `crates/server/src/rate_limit.rs`
- 证据：文档称写接口按 IP 限流（`tower-governor`）；实际仅登录限流。
- 修复方案：修正文档，或复用 `RateLimiter` 给写接口加限流。

---

## 值得保留、不要改的设计

- 事件日志服务端定序 + 草稿层 CRDT 的分层（02 已定 6）。
- 语义 presence 锚点 + 30 秒跟随退出 + 隐身不下发（02 已定 8）。
- 子会话无 composer、提供「让父 agent 转达」模板（04 §3.3），遵循 ACP v1 RFD。
- `idleBehavior: "promptRequired"` 的坚持（02 已定 11），两个 adapter 均已核实。
- 审批与提问「不自动拒绝、不超时消失」（02 已定 10）。
- Changes 不做 per-hunk keep/undo（02 已定 13）。
- 审批只阻塞发起的子 agent（04 §4）。

## 残余风险

- Delta 处于 public beta，v0.24.0 细节可能变化；01/05 中「Delta 没有权限系统」等表述应加时间限定。
- ACP v2 已引入 prompt 任意状态发送与 per-child prompt 能力；v1 相关决策未来需重评。
- 未做真实用户测试；presence 跟随、草稿块、审批卡密度的实际可用性未验证。
- 真实 agent 冒烟仅覆盖 Codex；Claude/opencode/pi 依赖 fake 契约。
