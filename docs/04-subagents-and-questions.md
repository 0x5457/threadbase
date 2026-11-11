# 子 agent、队列与标注 v0.2

配套文档：[01-requirements.md](01-requirements.md)、[02-open-decisions.md](02-open-decisions.md)、[03-tech-stack.md](03-tech-stack.md)。

v0.2 变化：取消右侧花名册，改为 Delta 风格的树形展示；补充队列与 steering 的事实与方案；补充评论/标注的做法；确认审批不自动拒绝。

---

## 1. 协议现状（已核实）

| 能力 | 状态 | 数据形态 |
| --- | --- | --- |
| 子 agent 会话 | ACP v1 的 **RFD 草案**，靠 `clientCapabilities.subagents: {}` 协商 | `subagent_update` 声明子会话，子事件用子 `sessionId` 在同一连接上流；可嵌套 |
| 子 agent 状态 | 同一 RFD | `state`: `running` / `requires_action` / `idle`，idle 带停止原因（`end_turn` / `cancelled` / `error`） |
| 子 agent 权限 | 同一 RFD | 请求走同一连接，受与父会话相同的权限检查；adapter 标注来源子会话 |
| 提问（elicitation） | **v1 稳定** | `elicitation/create`，`mode: form`（扁平 JSON Schema）或 `url`；响应 `accept` / `decline` / `cancel` |
| 队列 | **v1 无标准** | 由客户端实现；v2 草案才规定「prompt 可在任意状态发送，agent 插入或暂存」 |
| Steerinng | **扩展**，非标准 v1 | `_session/steering`，`_meta.steering = {supported: true}`；codex-acp 与 claude-agent-acp 都已实现 |
| 评论 / 标注 | **协议不提供** | 纯产品层，锚点与存储全部自己做 |

各 adapter 的落地情况：

| agent | 子 agent | 提问 | steering |
| --- | --- | --- | --- |
| Codex（`codex-acp`） | 能力协商后走原生子会话，否则退化为工具调用 | 表单式 elicitation，非阻塞，答案 steer 回当前 turn | 支持 |
| Claude Code（`claude-agent-acp`） | 原生子会话（Agent/Task） | 表单式 elicitation | 支持 |
| opencode | 通过 `_meta["opencode/child-session-updates"]` 上报 | 无特殊支持 | 未实现 |
| pi | 无 | 无 | 无 |

---

## 2. 数据模型

事件增加可选字段：

```rust
pub struct EventEnvelope {
    pub seq: u64,
    pub thread_id: ThreadId,
    pub subagent_id: Option<SubagentId>,
    pub parent_subagent_id: Option<SubagentId>,
    pub tool_call_id: Option<ToolCallId>,
    pub delivery: Option<Delivery>,   // injected | stopped_and_sent | queued | dropped
    pub payload: ThreadEvent,
}
```

`subagents` 表仍是花名册，只是不再单独占一栏面板，数据给树用：

| 字段 | 说明 |
| --- | --- |
| `id` / `thread_id` / `parent_id` | 会话标识与父子关系，支持任意嵌套 |
| `title` / `description` | 取自 `subagent_update`，可被后续 update 覆盖 |
| `status` | `running` / `requires_action` / `idle` / `cancelled` / `error` |
| `cwd` | 与 thread 根目录不同时，界面必须显示 |
| `can_cancel` | 是否声明了 `cancel` 能力 |
| `usage` | agent 上报的数字，**不做推断聚合** |
| `last_seq` | 排序与「跳到最新」 |

`subagent_update` 是 upsert：首次出现即建档，后续只打补丁，同时落一条 `subagent_updated` 事件，保证重连与重启能重建。

---

## 3. 展示：Delta 风格的树（就是左栏）

树不是第三列，也不是右侧面板。Delta 的左栏本身就是树：Project → Thread → Subthread。我们照抄这个层级；右栏只保留 Changes、Approvals、Output、Comments、Files 五个标签（v1 无 Terminal/PTY）。

### 3.1 树的位置与结构

- 位置：最左栏。第一层是 `Pinned` 与 `Projects` 分组；Project 下直接挂 thread；thread 下有子 thread 时显示 chevron，展开后子 thread 缩进一层，可继续嵌套。
- 行内元素：状态点或色条、标题、chevron、参与者头像；共享/可见性图标、agent 图标。
- 展开策略：父节点默认展开最新 10 个子 thread，底部显示 `Show N more`；数量由设置项 `Subthreads Per Thread` 控制。
- 过滤：`Plain` / `Subagents` 两个过滤（Delta 另有 Reviews、Merge，我们无此概念，不照搬）。
- 选择器：`View All Subthreads`（`⌘/Ctrl + J`）弹出可过滤的列表，用于子 thread 很多时快速跳转。
- 打开方式：单击设为当前视图；`⌘/Ctrl + 单击` 或行内 `Open in Tab` 在新标签页打开。
- 顶栏保留计数：`3 运行中 · 1 等待审批`。

### 3.2 选中与切换

- 点节点切换中间时间线，URL 变为 `/p/:projectId/t/:threadId/s/:subagentId`，可直接分享和刷新。
- 选中子 thread 时，中间顶部出现面包屑：`thread › Test investigator › Windows 失败排查`，右侧有「回到父 thread」。
- 派遣发生的时刻仍在父时间线渲染一张内联卡片（标题、状态、当前动作、用量），点卡片等于点树节点。卡片是入口，不是第二套导航。
- agent 之间的消息折叠为 `Sent Agent Message` / `Received Agent Message` 事件，展开可读。ACP 的 `session_message` 正好对应这个形态。
- 键盘：`⌘/Ctrl + ↑/↓` 在树内移动，`Esc` 回父节点。

### 3.3 子 thread 能否发言：与 Delta 的已知差异

Delta 自建 harness，子 thread 可以发消息、可以 steer。ACP v1 的子 agent RFD 明确禁止客户端向子会话发 prompt，只允许在子会话声明 `cancel` 时单独取消。

所以：

- 默认不显示输入框，底部只有「取消该子 agent」（仅当 `can_cancel`）与「回到父 thread 发言」。
- 需要让子 agent 继续时，提示用户「在父 thread 里让 agent 转达」，并预填一句模板。
- 若将来 adapter 暴露了 steer 子会话的扩展，再打开输入框，作为能力增量，不改布局。
- 子会话可以写文件、跑命令，不受「只读」限制；受限的只是用户不能给它派活。界面上不要暗示它是只读的。

### 3.4 退化渲染

未声明 `subagents` 能力或 agent 不支持时：

- 树里只有根节点，不显示空的子 agent 分组。
- 子过程作为普通工具调用卡片渲染，标注「子过程（无独立会话）」。
- 不出现空面板、不出现永久加载态。

这条路径 M1 就要实现，因为它是默认路径。

---

## 4. 审批路由

- **统一收件箱**：所有审批进 thread 级收件箱，不为子 agent 单独开列表。
- **来源链**：审批项显示 `子 agent：Test investigator › 第 2 层`，点进去跳到该子会话对应位置。
- **阻塞范围**：只阻塞发起的子 agent。界面必须明示「其余 2 个子 agent 仍在运行」。
- **不自动拒绝**：无人处理就一直等待。thread 顶栏显示「等待审批」，收件箱计数持续可见；审批项不会超时消失。
- **一律允许的作用域**：`本 thread + 该 agent + 该工具/命令前缀`，对根会话与该 thread 下所有子会话生效。
- **协议边界**：协议层 `allow_always` 的记忆范围由 agent 决定；产品的 thread/session 粒度由 `approval_rules` 强制约束，两者不一致时以产品规则为准。
- **例外**：子会话 `cwd` 与 thread 根目录不同时不继承，必须单独确认，并显示工作目录。
- **派遣本身**：父会话「允许创建子 agent」属于父会话的审批，出现在根层级，不标子来源。
- **取消 turn**：该 turn 下所有未处理审批自动失效，并留一条「随 turn 取消」的记录。

### 审批项要素

发起者（哪个子 agent）、要做什么、完整参数、涉及路径、工作目录、可选项（本次允许 / 本会话允许 / 本 thread 一律允许 / 拒绝）。

---

## 5. 提问（elicitation）

### 5.1 呈现

- **卡片，不是模态框**。并行提问不能堆叠。
- 位置：带 `toolCallId` 时挂在对应工具调用下，否则挂时间线末尾，同时进收件箱。
- 内容：发起 agent 与子 agent 标识、`message`、表单字段、三个动作。
- 表单按 `requestedSchema` 扁平结构渲染（string / number / boolean / enum / 基础数组），有默认值时预填，提交前允许修改。

### 5.2 三个动作

| 动作 | 含义 | 文案 |
| --- | --- | --- |
| `accept` | 提交或同意 | 提交 |
| `decline` | 明确拒绝 | 拒绝 |
| `cancel` | 关闭但不表态 | 取消 |

不能合并成一个「关闭」，否则 agent 无法区分"不同意"与"没看见"。

### 5.3 URL 模式

v1 未启用协议能力（`elicitation_url: false`），渲染路径已实现；启用后要求：显示完整 URL 与主机名、显式点击才打开、新标签页、禁止 iframe 与预取、收到 `elicitation/complete` 才收敛。

### 5.4 多人同时在线

- 任何 editor 都可回答；服务端只接受第一个有效响应，其余客户端立即只读并显示「由 X 于 14:32 回答」。
- 回答人写入事件日志与审计。
- 提问因 turn 取消而失效时，卡片消失并留一条记录。

### 5.5 非阻塞提问

Codex 的非阻塞提问在协议上仍是 `elicitation/create`，adapter 立即返回并把答案 steer 回 turn。界面加「非阻塞」标记，提示「回答会在当前 turn 中途注入」。不要因为非阻塞就降级成普通消息。

### 5.6 安全

表单模式禁止收集密码、API Key、Token、私钥、恢复码。看到可疑字段名（`password`、`token`、`secret`、`api_key`）时警告并拒绝提交。回答内容按不可信输入处理。

---

## 6. 队列与 steering

### 6.1 三种投递方式

| 方式 | 触发 | 行为 | 默认 |
| --- | --- | --- | --- |
| 插话（steering） | agent 声明 `_meta.steering.supported` 且 turn 运行中 | 并入当前 turn，不打断 | 能插话时默认 |
| 停止并发送 | 用户显式选择 | `session/cancel` 后立即 `session/prompt` | 非默认；选择时提示会取消当前 turn 与未处理审批 |
| 排队 | 不能插话且 turn 运行中（默认）；或用户显式选择 | 进入 thread FIFO，turn 结束后发送 | 不能插话时默认 |

发送按钮右键菜单按能力显示这三项；界面文案：`插话` / `停止并发送` / `排队`。

### 6.2 队列在服务端

- 每 thread 一个 FIFO，由服务端持有；持久化到 `queued_inputs`，刷新、换设备、服务重启都不丢。
- 界面显示「排队 2 条」，可撤回（editor 及以上）。
- turn 被取消时队列保留，每条标注「上一轮已取消」，由用户逐条决定继续或撤回。
- 队列中的消息计入事件日志（`delivery: queued`），发送后再落一条 `injected`。

### 6.3 steering 是扩展，但两家都实现了

- 方法：`_session/steering`，能力从 `initialize` 的 `_meta.steering.supported` 读取。
- 结果四种：`injected`（并入当前 turn）、`startedNewTurn`（无 turn 时自己起一轮）、`promptRequired`（无 turn 可接收，内容留给客户端）、`failed`。
- 我们**始终**带 `_meta: { steering: { idleBehavior: "promptRequired" } }`，保证没有 running turn 时 adapter 不会自己起一轮；拿回 `promptRequired` 就默认转入队列（不自动取消 turn），用户可显式改为「停止并发送」。
- 同一会话的 steering 按到达顺序串行，不同会话并行。

### 6.4 反馈

每次投递都要有可见结果：`已并入` / `已停止上一轮并发送` / `已排队（第 2 位）` / `失败（原因）`。事件里的 `delivery` 取 `injected` / `stopped_and_sent` / `queued` / `dropped`。

### 6.5 与 v2 的关系

ACP v2 草案把「prompt 可在任意状态发送、agent 插入或暂存」写进规范，并允许 agent 声明 steering/queueing 行为。我们不跟进 v2，但事件模型里的 `delivery` 字段与 v2 的语义对齐，将来迁移不需要改前端。

---

## 7. 评论与标注

### 7.1 ACP 不提供这件事

评论、标注、@提及、锚点全部是产品层能力。协议只提供会话事件流；不要在 ACP 里找对应方法。

### 7.2 锚点

| 类型 | 锚点数据 | 说明 |
| --- | --- | --- |
| 消息 | `eventId` / `messageId` | agent 或人的消息 |
| 工具调用 | `toolCallId` | 挂在某次工具调用上 |
| 提问 | `elicitationId` | 挂在卡片上，但回答仍走表单，两者不混 |
| 文件 | `path` + `line` | 只读文件视图 |
| diff | `path` + `hunkId` + `line` + 上下文哈希 | 文件变化后可能漂移 |

漂移处理：锚点带上下文哈希，找不到时降级为文件级评论并在界面标注「原位置已变化」，不静默错位。

### 7.3 不修改 agent 内容

事件日志不可变。评论是挂在锚点上的独立实体，渲染在边栏/行内边距，不写回 agent 消息。

### 7.4 评论何时到达 agent

Delta 的做法是批处理，不是单条手动推送：

- 评论默认处于 pending 状态，**随下一条消息一起提交**，agent 在同一次回复里处理；也可以**单独提交**一条评论链，不带消息；单独提交会作为一条独立消息注入 agent。
- agent 的回复与评论**双向链接**：从评论跳到回答，也能从回答跳回评论；流式输出期间评论回复就显示在回答旁边。
- 文件与 diff 评论：选中代码直接输入即建评论（评论模式默认开启）；可切换为编辑模式；提交后折叠成 gutter 标记，`⌘/Ctrl+⇧+M` 重新打开讨论。
- 单条评论链可以「发给新 fork」（Delta 的 Send Comment to Fork，`⌘/Ctrl+⇧+Enter`），我们对应为新建 thread 并把该评论链作为初始上下文。
- 锚点漂移时降级为文件级并提示，不静默错位。

与之前 v0.1「必须显式让 agent 看」的差别：**不要求逐条授权，但也不在 turn 运行中实时注入**。评论跟着下一次提交或单独提交进入上下文，这是批处理，不是实时流。

### 7.5 多人

- 随消息提交或单独提交的评论链都会进入 agent 上下文；评论不改变 thread 状态；pending 评论对同 thread 成员可见。
- @提及产生通知；被提及的 agent 等价于「随下次提交处理」。
- 评论的处理状态（未处理 / 已处理 / 已解决）由人维护，agent 的回复不会自动标记解决。

---

## 8. 能力协商顺序

| 阶段 | 声明 | 效果 |
| --- | --- | --- |
| M1 | 不声明 `subagents` | 全部退化为工具调用渲染，树只有根节点 |
| M2 | 声明 `elicitation.form` | Codex 的提问可用；URL 模式不声明 |
| M2 | 读 `_meta.steering` | 有则启用插话，无则只用排队 |
| M3 | 声明 `subagents: {}` | 树、状态、聚焦视图启用 |
| M4 | 声明 `elicitation.url` | 外部授权流程可用 |

声明是 feature flag，可按 adapter 单独关闭。

---

## 9. 验收标准

1. 同时 3 个子 agent：树显示 3 个节点与状态；批准其中一个不阻塞另外两个，界面明示。
2. 嵌套两层：缩进与面包屑正确，事件不在父层重复。
3. 子 agent 崩溃：节点显示错误与停止原因，主 thread 可继续。
4. 断线重连：树与状态从快照恢复；子会话 transcript 是否可加载取决于 adapter 是否上报子会话事件。
5. 未声明子会话能力：树只有根节点，退化卡片可展开。
6. 队列：turn 中发 3 条，按序执行，界面显示位置；撤回一条后不再发送。
7. steering：中途插话显示「已并入」；不能插话时默认「排队」；返回 `promptRequired` 时默认转入队列，用户可显式改为「停止并发送」。
8. 提问：表单校验、可修改、三动作语义正确、多人首个响应生效。
9. 评论：行内锚点、漂移降级、单独提交的评论链能被 agent 正确理解。
10. 审批：来源链、工作目录、作用域与例外正确；无人处理时一直等待，不自动消失。

---

## 10. 已确认

1. 不做右侧花名册，用 Delta 风格树形展示。
2. 审批不自动拒绝，无人处理就一直等待。
3. URL 模式 v1 未启用协议能力，待后续启用。
