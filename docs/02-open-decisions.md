# 待确认决策 v0.2

配套文档：[01-requirements.md](01-requirements.md)、[03-tech-stack.md](03-tech-stack.md)。

状态标记：**已定** = 已确认，不再讨论；**待定** = 需要拍板；**默认** = 没有异议就按建议执行。

---

## 已定 1：部署形态（原 D1）

**结论**：单机多用户。一个服务进程跑在有代码的机器上，多人通过浏览器访问同一个页面。该机器离线则 thread 不可访问。**访问范围仅限 Tailscale 网络，不暴露公网。**

直接后果：

- 必须有账号体系与 thread 级角色，不能只用一个访问令牌。
- agent 凭据属于机器，不属于用户。所有参与者共用这台机器上的 Codex 登录，界面必须固定展示当前 agent 身份。
- 不做 relay、不做离线副本；事件日志只有一份权威副本。草稿层的 CRDT 只在单个服务进程内收敛，不是跨机器同步。
- 服务默认只绑 `127.0.0.1`；远程访问由 `tailscale serve` 终结 TLS 并转发到回环端口，公网不可达。

为什么不是其他形态：

| 形态 | 不选的原因 |
| --- | --- |
| 各自本地 + 中心 relay | 工作量 3 到 5 倍，需要账号、云存储、权限与冲突设计；当前需求不需要 |
| 纯 P2P / CRDT 同步 | 排序与审批语义难保证，与「服务端唯一写者」冲突 |

---

## 已定 2：后端语言（原 D2 的一部分）

**结论**：Rust。

理由、代价与依赖清单见 [03-tech-stack.md](03-tech-stack.md) 第 2 节。关键三点：

1. `agent-client-protocol` 5.0 是上游作者维护、Zed 生产使用的 ACP 实现。
2. 单二进制分发，用户不需要为服务端装 Node。
3. 草稿层要 CRDT，权威实现在 Rust（`yrs`、`loro`）；Node 方案只能靠 WASM 或另一个进程。

---

## 已定 3：首发 agent（原 D4）

**结论**：Codex。

- adapter：`@agentclientprotocol/codex-acp`，锁定版本。
- 模式映射：`read-only` / `workspace-write` / `agent` / `agent-full-access` / `permission-profile:<id>`。
- 第二个接 Claude Code，用来验证「接新 agent 不需要改核心」。

---

## 已定 4：开源（原 D5）

**结论**：开源，Apache-2.0。

对结构的影响：crates 与 web 分离清晰；依赖用 `cargo-deny` 做许可审查；文档中文优先，接口注释与提交信息英文。

---

## 已定 5：平台（原 D2 的一部分）

**结论**：v1 支持 macOS 与 Linux，同一套 Rust 代码、各自产出单二进制。Windows 不承诺。

---

## 已定 6：草稿模型（CRDT，限定在草稿层）

**结论**：DeltaDB 基于 CRDT，但 CRDT 只在文本层，不在线程事件层。我们照此分层：**草稿块用 CRDT，消息与审批仍由服务端定序。**

证据：

- `deltadb.md`：`DeltaDB is built on CRDTs, loosely based on diamond-types and adapted to work with Git`。
- 博客：`as you type, changes sync to your collaborators and to the server`；编辑携带 base version（Git blob 或父 delta）。
- UI 层是 per-author 草稿块：`Each participant composes their own draft, attributed to them`；发送时 `submits every trailing draft under its own author`。

实现：

- 每条 thread 一个 `Y.Doc`；块列表是 `Y.Array`，块是 `Y.Map { author, text: Y.Text, createdAt }`。
- 块内字符级合并；块的新建、删除、编辑他人块都由 CRDT 收敛，不需要 `409` 或服务端 position。
- 发送是服务端串行事务：读 Y.Doc → 取末尾连续块 → 生成消息事件（`submittedBy` + `authors[]`）→ 删除块 → 广播。
- 客户端 `yjs` + `y-codemirror.next`，块内光标用 awareness；服务端 `yrs` + `y-sync`。
- 持久化：`draft_updates` 增量 + `draft_snapshots` 定期快照；发送成功后清空。

边界：

- 不对事件日志用 CRDT；消息顺序、审批、评论仍是 `seq` 定序。
- 不对文件内容用 CRDT（v1 没有内置编辑器）。
- CRDT 状态不写进 `events` 表。

详细设计见 [03-tech-stack.md](03-tech-stack.md) 第 5 节、[05-ui-spec.md](05-ui-spec.md) 第 5.2 节。

---

## 已定 7：Codex 接入路径

**结论**：v1 只走 ACP（`codex-acp`），不直接实现 `codex app-server` 协议。

- 原因：一套接入协议是产品前提；直接讲 app-server 会立刻产生第二条适配链。
- 逃生口：`Harness` trait 已隔离，如果 adapter 长期成为瓶颈，再加 `NativeCodexHarness`。

---

## 已定 8：焦点共享与跟随

**结论**：做。成员共享语义焦点（project、thread、面板、文件、行、hunk、消息 id），可被他人跟随；颜色按用户 id 稳定映射，与 Delta 的多人光标一致。

- 通道：WS 上的 `presence`，JSON、无 `seq`、不落库、不重放。
- 节流：客户端 10 Hz，服务端按 `(thread, user)` 合并到 20 Hz 广播。
- 锚点必须是语义的，不传像素坐标，保证不同窗口尺寸下跟随不错位。
- 跟随是纯客户端状态，本地输入或点击即退出。
- 隐身开关只影响焦点，不影响在线状态。

这是独立于 CRDT 的一套机制。两者在界面上共用颜色与标记组件。

---

## 已定 9：子 agent 展示与审批路由

**结论**：树就在左栏（Project → Thread → Subthread），不设右侧花名册；统一收件箱，审批只阻塞发起的子 agent。

- 左栏结构照 Delta：`Pinned` / `Projects` 分组，Project 下直接挂 thread，带 chevron 的子 thread 缩进嵌套；默认展开最新 10 个，底部 `Show N more`。
- 选择器 `View All Subthreads`（`⌘/Ctrl + J`）；`⌘/Ctrl + 单击` 在标签页打开。
- 派遣时刻仍在父时间线渲染内联卡片，卡片是入口，不是第二套导航。
- 状态四态：`running` / `requires_action` / `idle` / `error`。
- **与 Delta 的已知差异**：Delta 的子 thread 可以发消息、可以 steer；ACP v1 RFD 禁止向子会话发 prompt。我们的子会话视图不提供输入框，只能取消（若声明 `cancel`）或回父 thread，并在界面给出「让父 agent 转达」的模板。
- 「一律允许」作用域：本 thread + 该 agent + 该工具/命令前缀；子会话 `cwd` 与 thread 根目录不同时**不继承**，需单独确认。
- 退化渲染：不声明 `subagents` 能力时，树只有根节点，子过程包成工具卡片，M1 就走这条路径。
- 能力声明按阶段开启：M1 不声明，M3 声明 `subagents: {}`；可按 adapter 单独关闭。

详细设计见 [04-subagents-and-questions.md](04-subagents-and-questions.md)。

---

## 已定 10：提问交互（elicitation）

**结论**：卡片式、非模态、进收件箱；三个动作严格对应协议的 `accept` / `decline` / `cancel`。

- 表单按 `requestedSchema` 渲染，提交前允许修改；可疑敏感字段（password/token/secret）拒绝提交。
- 多人只接受第一个有效响应，其余客户端只读并显示回答人。
- Codex 的非阻塞提问加「非阻塞」标记，回答会 steer 回当前 turn。
- URL 模式推迟到 M4；M2 只声明 `elicitation.form`。
- **不做自动拒绝**：无人处理就一直等待，直到有人处理或 turn 被取消。thread 顶栏持续显示「等待审批」。

详细设计见 [04-subagents-and-questions.md](04-subagents-and-questions.md)。

---

## 已定 11：队列与 steering

**结论**：三种投递方式，按能力与用户选择决定；默认优先「插话」，其次「停止并发送」，最后才是「排队」。

- ACP v1 **没有**标准队列与 steering；v2 草案才规定 prompt 可在任意状态发送。
- `_session/steering` 是预标准扩展，`codex-acp` 与 `claude-agent-acp` 都已实现，能力从 `_meta.steering.supported` 读取。
- 顺序：
  1. 支持 steering 且 turn 运行中 → **插话**（并入当前 turn）；
  2. 不支持 steering 且 turn 运行中 → **停止并发送**（cancel 后立即发送），这是 Delta 的行为；
  3. 用户显式选「排队」→ 进入 thread 队列。
- 我们始终带 `idleBehavior: "promptRequired"`，避免 adapter 在没有 running turn 时自己起一轮；拿回 `promptRequired` 就转入队列走正常 `session/prompt`。
- 队列非默认，但完整：可撤回、可置顶（editor 及以上）；刷新或换设备不丢；turn 取消后保留并标记「上一轮已取消」。
- 每条输入都记录投递结果：`injected` / `stopped_and_sent` / `queued` / `dropped`。

详细设计见 [04-subagents-and-questions.md](04-subagents-and-questions.md) 第 6 节、[05-ui-spec.md](05-ui-spec.md) 第 5.3 节。

详细设计见 [04-subagents-and-questions.md](04-subagents-and-questions.md) 第 6 节。

---

## 已定 12：评论与标注

**结论**：评论是产品层能力，ACP 不提供；锚点要语义化；投递方式照 Delta 做批处理。

- 锚点：消息 / 工具调用 / 提问 / 文件行 / diff hunk；diff 锚点带上下文哈希，漂移时降级为文件级。
- 选中文本直接输入即建 draft 评论，不切换模式；`⌘/Ctrl + Option/Alt + M` 也可。
- **pending 评论随下一条消息一起提交**，agent 在同一次回复里处理；也可单独提交一条评论链。
- agent 回复与评论双向链接；流式期间评论回复跟随回答展示。
- 文件/diff 评论提交后折叠为 gutter 标记；单条评论链可以「发给新 fork」。
- 不在 turn 运行中实时注入；评论跟着下一次提交进入上下文。
- 评论不触发 agent、不改变 thread 状态；@提及产生通知。

详细设计见 [04-subagents-and-questions.md](04-subagents-and-questions.md) 第 7 节。

---

## 已定 13：UI 规格以 Delta v0.24.0 为准

**结论**：外壳、左栏树、时间线密度、Changes 审阅、Share、跟随、Inbox 照 Delta 做；逐屏规格见 [05-ui-spec.md](05-ui-spec.md)。

- 身份在标题栏，分支与 worktree 在底部状态栏（`Project Settings`，`⌘⌥O`）。
- 时间线密度靠**折叠摘要行**（`Ran 3 commands, thought, read a file ›`），不是卡片样式。
- 子 thread 用树 + tab，不用卡片网格；`⌘J` 打开一级子级选择器。
- Changes 用 `Seen` 复选框 + `3/17` 进度，不做 per-hunk keep/undo。

---

## 已定 14：五处刻意偏离 Delta

| # | 偏离 | 理由 |
| --- | --- |
| 1 | 有审批 UI | Delta 官方明确「没有 agent 权限系统」；ACP agent 会请求权限，无 UI 会卡死且有安全风险 |
| 2 | project 为一级维度 | 产品定义要求；Delta 的 organization 一级不适用 |
| 3 | 子 thread 无 composer | ACP v1 RFD 禁止向子会话发 prompt；提供「让父 agent 转达」模板 |
| 4 | 无 Reviews / Merge 两类 thread | 不自建 harness；先做单 thread 内审阅 |
| 5 | 分享只有两档 | 不暴露公网，`Anyone with the link` 没有意义 |

除这五处，其余交互、文案结构与视觉层级以 Delta 为准。

---

## 已定 15：发言模式与分享级别（原待定 1）

**结论**：v1 只有「直接发送」，不做建议队列；分享级别简化为两档。

- 所有 editor 都可以直接发送、插话、停止并发送；没有 driver，没有「接管」按钮。
- 不做「非 owner 消息先进待接受队列」的模式：访问范围已经限定在 tailnet 内，参与者是可信成员；再加一层审批只会拖慢协作。
- 分享级别：`仅邀请` / `整个工作区`。Delta 的 `Anyone with the link` 依赖公网可访问，我们不暴露公网，v1 不做。
- 若将来要对外开放，再引入「建议队列 + 链接访问」作为一个整体能力，不单独上线。

---

## 已定 16：块内实时光标

**结论**：开。草稿块内显示协作者光标与选区。

- 由 `y-protocols/awareness` 提供相对位置，随 y-sync 通道走。
- 颜色与全局 presence 共用 `hash(user_id)` 映射。
- 只显示「正在编辑这条草稿」的块级状态与块内光标，不做跨块的字符级同步。
- 隐身开关同时作用于焦点与块内光标。

---

## 已定 17：远程访问走 Tailscale

**结论**：只支持 tailnet 访问，不做公网暴露、不做内置 TLS。

形态：

- 服务只绑 `127.0.0.1:7777`。
- 远程访问用 `tailscale serve --bg 443 http://127.0.0.1:7777`，由 Tailscale 提供 MagicDNS 域名与有效证书；浏览器访问 `https://<machine>.<tailnet>.ts.net`。
- 访问控制两层：Tailscale ACL 决定谁能连到这台机器；应用账号决定角色（owner / editor / viewer）。
- 因为前端始终是 HTTPS，Cookie 可以稳定使用 `Secure` + `HttpOnly` + `SameSite=Lax`。
- 不实现端口转发、frp、ngrok 之类的公网暴露方式；文档里明确写「不要把端口开到公网」。
- 若 `tailscale serve` 透传身份头（`Tailscale-User-Login`），可作为账号绑定的一种方式；不作为 v1 的前置依赖。

---

## 已定 18：身份与登录（Tailscale 优先）

**结论**：优先用 Tailscale 身份免密登录，本地账号作为兜底与离线入口。

- 服务只绑 `127.0.0.1`，请求经 `tailscale serve` 转发；只有来自回环代理的请求才读取身份头。
- 身份头（登录名、显示名、头像）具体名称与透传行为**以实测为准**，文档不写死；实现时先用一个最小探针页面确认。
- 首次出现的 tailnet 用户自动建档，默认 `viewer`；管理员可提升为 `editor` / `owner`。
- 保留一个本地管理员账号，用于：首次启动引导、Tailscale 不可用时、身份头缺失时。
- 角色与 thread 成员关系仍存在本地库；Tailscale ACL 只负责「谁能连上这台机器」，应用层再做「谁能做什么」。
- Cookie 仍用 `HttpOnly` + `Secure`（前端始终 HTTPS）+ `SameSite=Lax`，CSRF 校验不变。
- 如果身份头不可用，退化为本地账号密码登录，不阻塞使用。

---

## 已定 19：首发平台与分发

**结论**：macOS + Linux 同时首发，走 GitHub Releases。

- 产物：`threadbase` 单二进制（内嵌 web 资源），macOS aarch64 / x86_64，Linux x86_64 / aarch64。
- 分发：GitHub Releases 附件 + `curl -fsSL ... | sh` 安装脚本；暂不做 Homebrew tap。
- macOS 公证：若首发前拿不到 Developer ID，Release 说明里写清 Gatekeeper 的处理方式，不假装已公证。
- 升级：`threadbase upgrade` 从 Releases 拉取对应平台产物并校验校验和；数据库迁移在启动时执行。
- CI：macOS 与 Linux 各跑构建与测试；发布由 tag 触发，产物附 SHA256。

---

## 下一轮要定的规划项（不写代码）

1. 视觉 token 与组件清单：间距、字号、圆角、色板落地成 Tailwind theme。
2. M1 的验收脚本：两个浏览器、一条 thread、一次审批、一次断线重连。
3. ACP 适配的 spike 清单：初始化、prompt、permission、cancel、steering、子会话能力探测。
4. Tailscale 身份头的最小验证（一个本地探针页）。

---

以上都是规格工作，不涉及代码。实现等明确指令后再开始。
