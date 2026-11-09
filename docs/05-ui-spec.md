# UI 规格 v0.1（对齐 Delta v0.24.0）

依据：[delta-ui.md](research/delta-ui.md)（463 行调研，含逐条证据与未验证项）、[01-requirements.md](01-requirements.md)。原始截图与文档在 [research/_raw/](research/_raw/)。

标注：**照抄** = 与 Delta 一致；**偏离** = 有理由的不同；**未验证** = Delta 也无证据，先不做承诺。

---

## 1. 设计原则

1. **密度靠折叠摘要行，不靠卡片样式**。时间线的观感主要来自 `Ran 3 commands, thought, read a file ›` 这类单行摘要，不是工具卡的装饰。
2. **身份放标题栏，分支放状态栏**。thread 标题与参与者头像在顶部；project、分支、worktree 在底部状态栏的 Project Settings 里。
3. **树是唯一导航**。project → thread → subthread 在左栏缩进，没有第二套子 agent 面板，也没有卡片网格。
4. **评论是选中即打字**，不是一个模式切换按钮。
5. **四处刻意偏离**（第 12 节）：审批 UI、project 一级维度、子 thread 的 composer、Reviews/Merge 两种 thread 类型。

---

## 2. 外壳

```
┌───────────────────────────────────────────────────────────────┐
│ ▣  ⌕        thread 标题            ● ● ●   [分享] [审批] [▤]  │ 标题栏
├──────────────┬────────────────────────────────────────────────┤
│ Pinned       │ tab: Thread | Changes +12 −3 | Files           │
│  ├ thread    │ ┌────────────────────────────────────────────┐ │
│ Projects     │ │ 时间线（单列，头像列 56px）                  │ │
│  ├ project   │ │ ...                                        │ │
│  │  ├ thread │ └────────────────────────────────────────────┘ │
│  │  │  └ sub │ composer（与时间线同宽）                       │
├──────────────┴────────────────────────────────────────────────┤
│ ⌘ project  branch ⌄        +694 −85      3 运行中 · 1 等待审批 │ 状态栏
└───────────────────────────────────────────────────────────────┘
```

**照抄**

- 标题栏横跨整宽，左栏与主区在标题栏下才分栏。
- 标题栏左：侧栏开关、全局搜索；中：thread 标题 + 参与者头像（重叠小圆）；右：动作簇 + 右栏开关。
- 主区时间线单列，正文有三档宽度：`default` / `full` / `custom`（拖动 ruler）。
- 右栏是抽屉，默认面板为 Changes；`Shift+Esc` 关闭。
- 左栏可整列折叠。

**偏离**

- Delta 标题栏右侧是 `Reviews / Merge / Share / 右栏开关`。v1 没有 review thread 与 merge 流程，改为 `Share / Approvals / 右栏开关`，顺序与图标风格保持一致。
- Delta 的分支与 worktree 在底部状态栏（`Project Settings`，`⌘⌥O`）。照抄位置，但内容只保留 project、分支、`Work In` 选择、远端状态。

---

## 3. 左栏树

**结构**

```
Pinned
  ├ thread 行（可带子级）
Projects
  ├ project 行（folder 图标，右键菜单）
  │   ├ New Thread
  │   ├ thread 行
  │   │   ├ subthread（缩进 +1，细竖引导线）
  │   │   │   └ 孙 thread（任意深度）
  │   └ Show 7 more
Threads（无 project 的 thread）
底部：头像 · 名字 · 在线点 → 用户菜单
```

**照抄**

- 行结构：`chevron（有子级才显示） · kind 图标 · 标题 · （副标题 = project） · 右侧指示（头像 / 未读点 / 书签）`。
- 状态表达：运行中在折叠的父行显示 running indicator；未读是行尾蓝点；被标记的标题更亮更粗；色条 = 该 thread 的第一个彩色标签。
- 子 thread 颜色不继承父级。
- kind 图标：眼睛（review）、分支（merge）、对话气泡（普通 subthread）。v1 只保留气泡。
- 分组末尾 `Show N more`，默认 10，可设 25 / 50 / All。
- 空状态：project 下无 thread 显示灰字 `No threads yet`；路径失效显示警告与 `Locate…`。
- 过滤/排序：`Filter by`（Projects / Owners / Subthreads / Tags）、`Group by`、`Sort by`。v1 只做 `Subthreads: Plain / Subagents` 与 `Sort by: Last Response`。

**偏离**

- Delta 的一级维度是 organization → `Pinned / Projects / Threads`。Threadbase 是 **project 为一级维度**：project 列表常驻，thread 全挂在 project 下。保留 `Pinned` 与 `Threads`（无 project）两个分组。

**行高与缩进**：行高约 34–36 px，每层缩进 +1 级（约 16 px），引导线 1 px 低对比。数值为目测，最终由视觉稿定。

---

## 4. Thread 视图

### 4.1 头部与 tab

- 标题栏承载 thread 标题与参与者头像（照抄）。
- 窄屏/移动端另有 tab 条：`Thread` / `Changes +694 −85` / `Files`（照抄截图 21）。
- 桌面端 Changes 在右栏；打开文件或子 thread 时出现 tab 条（照抄）。

### 4.2 时间线元素顺序

1. 用户消息（圆形头像在左）。
2. 发送时合并进来的他人草稿：每条独立头像，不合并成一段。
3. agent 消息（品牌/字母头像）。
4. **折叠摘要行**（见 4.3）。
5. 特殊折叠行：`Thinking ›`、`Context Compacted ›`、`2 comments ›`。
6. 工具卡：`Apply patch` / `Read File` / `Edit File` + 文件路径 + chevron + 语法高亮 diff。
7. 终端块：`zsh ⌄` + 圆角描边内容 + 头像。
8. 行内评论卡：正文范围高亮 + 头像 + 回复堆叠。
9. 事件行：`Routed to Subthread`、`Sent Agent Message` / `Received Agent Message`、`Merged` / `Merge Failed`。
10. **审批卡**（我们新增，见第 9 节）。

### 4.3 折叠摘要行（观感第一来源）

动词短语固定，按发生顺序拼接，行尾 `›` 展开：

| 场景 | 文案 |
| --- | --- |
| 执行命令 | `Ran 1 command` / `Ran 3 commands` |
| 思考 | `thought` / `thought twice` |
| 读文件 | `read a file` / `read 2 files` |
| 调工具 | `Called 2 tools` |
| 取网页 | `fetched a URL` |
| 加载技能 | `loaded a skill` / `Loaded 2 skills` |

规则：

- 文本段落永远不折叠；工具、命令、读取、搜索才折叠。
- 展开后显示逐条工具卡，顺序与发生顺序一致。
- 单次工具的耗时与 token **不展示**（Delta 也没有，未验证）。

### 4.4 评论

- 选中正文或代码直接输入即建 draft 评论，不切模式；`⌘/Ctrl + ⌥/Alt + M` 同效。
- draft → pending（随下一条消息提交）→ delivered（收进 gutter，`⌘/Ctrl + ⇧ + M` 重开）。
- 多人草稿可见；发送时连同他人 pending 评论一起送出（照抄）。
- 评论卡左侧有作者色竖线；agent 回复与评论双向链接。
- 范围高亮用多色（按作者），`⌘/Ctrl + ⌥/Alt + V` 显隐评论。

### 4.5 导航轨

时间线右缘每个 turn 一个刻度；`⌘` + 点击加书签，`Option` + 滚轮逐 turn 跳。v1 可放到 P2。

---

## 5. Composer

### 5.1 控件行（从左到右，照抄顺序）

`上下文圆环` · `effort（💡 Medium）` · `模型名` · `runner（本机）` · `发送`。

- 上下文圆环点开是 Context Window popover：自动压缩、压缩阈值、用量明细。
- effort 循环：`Ctrl + '`。
- 模型选择器只展示 agent 上报的 `configOptions`，缺失时隐藏。

### 5.2 草稿模型：per-author blocks + 块内 CRDT

Delta 的 UI 模型是 per-author 草稿块；底层文本同步是 CRDT（DeltaDB）。两者并不矛盾，我们照此实现：

```
[draft · alice]  把超时改成读配置      ← 块内 Y.Text，可被他人同时编辑
[draft · bob]    顺便加个测试          ← 独立块，自己的作者与光标
        ↑ 两人都还没发送，agent 看不到
```

- **块列表**：`Y.Array`，块是 `Y.Map { author, text: Y.Text, createdAt }`。并发新建、删除、编辑他人块都由 CRDT 收敛。
- **块内文本**：`Y.Text`，字符级合并；两人同时改同一块不会互相覆盖。
- **光标**：`y-protocols/awareness` 提供块内相对位置，颜色与全局 presence 共用映射。
- **发送**：服务端串行事务，读 Y.Doc → 取末尾连续块 → 生成消息（`submittedBy` + `authors[]`）→ 删除块 → 广播。两人同时发送，第一个成功，第二个收到 `already_sent` 并保留本地副本。
- **持久化**：`draft_updates` 增量 + `draft_snapshots` 快照；发送成功后清空。

不需要 `409`、不需要服务端 position、不需要块级锁。文本之外的东西（消息顺序、审批、评论）仍然由服务端 `seq` 定序，不进 CRDT。

### 5.3 发送语义

| 场景 | 行为 | 与 Delta 的关系 |
| --- | --- | --- |
| 无 turn 运行 | 发送 | 照抄 |
| 有 turn 运行，agent 支持 steering | 默认「插话」：并入当前 turn | 偏离：Delta 没有 steering 概念 |
| 有 turn 运行，不支持 steering | 默认「停止并发送」：cancel 当前 turn 后立即发送 | 照抄 Delta |
| 用户显式选择「排队」 | 进入 thread 队列，turn 结束后发送 | 偏离：Delta 没有队列 |

- `⌘/Ctrl + Enter` 发送，`Enter` 换行；关闭 Send With Modifier 后 `Enter` 发送。
- 发送按钮右键菜单：`插话` / `停止并发送` / `排队`（按能力显示）。
- 发送后草稿块清空，事件记录 `submittedBy` 与 `authors[]`。

### 5.4 @ 与终端

- `@` picker 四类：Files & Directories / Threads / Users / Terminals，支持 `@file:` `@thread:` `@user:` `@terminal:`。
- `!` 行首开内联终端；`@!` 打开 Terminals picker（`New Terminal in <project>` / 已有会话）。
- 图片可直接粘贴进消息与评论。

---

## 6. 子 agent / 子 thread

**照抄**

- 左栏缩进嵌套，任意深度，细竖引导线。
- 点击 = 开为主视图；`⌘/Ctrl + 点击` = 开 tab；行 hover 有 `Open in Tab`。
- tab 内 `⌘/Ctrl + ⌥/Alt + Enter` = 升为主视图，`⌘/Ctrl + ⌥/Alt + U` = 回父级。
- `⌘/Ctrl + J` 打开 `View All Subthreads`（只列一层直接子级，可过滤）。
- 派遣时刻：父时间线里的那条消息变为只读，带 subthread badge 与 `Route to subthread`。
- agent 互发消息折叠为 `Sent Agent Message` / `Received Agent Message`，点击跳到对方 thread。
- 聚焦子 thread 时 `Escape` 只停这一个。
- 父行折叠时显示 running indicator。

**偏离**

- Delta 的子 thread 是普通 thread，**有自己的 composer，可以直接接管**。ACP v1 的子 agent RFD 禁止客户端向子会话发 prompt。
- 因此子 thread 视图底部只有：`取消该子 agent`（仅当 `can_cancel`）、`回父 thread`，以及一句预填模板「让父 agent 转达」。
- 若 adapter 未来暴露子会话 steer 扩展，再打开 composer，布局不变。

**未验证**：子 thread 折叠时 running indicator 的具体形态，先做成 2 px 脉冲点。

---

## 7. Changes 审阅

**照抄**

- 右栏默认面板；`⌘/Ctrl + ⇧ + D` 或点状态栏增删计数打开。
- 比较基线四档：`origin/main`、`Last Commit`、`Last Turn`、某个 commit；默认档可配。
- 文件行：added / modified / deleted / renamed，重命名显示旧路径。
- 每文件 `Seen` 复选框，勾选后折叠；文件再次变更自动清除。快捷键 `⌘/Ctrl + ⌥/Alt + Y`。
- 头部进度 `3/17`；`Display Options` 里 `Mark All as Seen`（`⌘/Ctrl + ⇧ + Y`）、`Collapse All Files`（`⇧Tab`）。
- 文件间切换 `⌘[` / `⌘]`；行号栏上/下 hunk 按钮。
- 行内评论同上；提交后收进 gutter。
- **没有 per-hunk keep / undo**：替代是 `Revert Conversation to Cursor`、切基线看「上一轮之前」、`Pull Changes`。

**偏离**

- 我们没有 DeltaDB，多来源 checkout 的比较器简化为：当前 worktree、基线分支、某个 commit。协作者 checkout 的比较列进 P2。
- Merge 结果卡与 CI 状态列进 P2。

---

## 8. 协作

**Share 弹层（两档）**

- 两档：`仅邀请` / `整个工作区`；只有 owner 能改。
- 不做 `Anyone with the link`：v1 只在 tailnet 内访问，没有公网入口，链接不会给到网络外的人。
- 邀请按邮箱或 `@username`，一次最多 10 人，14 天过期，被邀请者为 editor。
- 复制链接不关闭弹层；私有 thread 就地警告「链接不授权」。
- 切到 `Only people invited` 不撤销已加入的人。

**跟随**

- 点头像 → `Follow`；`Ctrl + ⌥ + ⌘ + F` 循环跟随下一位协作者。
- 跟随跨越 thread、子 thread、tab、图片缩放；本地输入或点击即退出。
- 焦点锚点是语义的（thread、文件、行、hunk、消息 id），不传像素。

**Inbox**

- `⌘/Ctrl + ⇧ + J`；`Unread / All` 两档 + 计数；条目 = 头像 + 谁 @ 了你 + 相对时间 · thread 标题。
- 打开 thread 即清该 thread 的未读。

**Copy Link to Selection**

- 选中任意 span/行，`⌘/Ctrl + ⌥/Alt + L`；打开后目标高亮、周边变暗，`Escape` 退出。

---

## 9. 审批（唯一必须自研的主结构）

事实依据：Delta 官方文档明确「Delta does not have an agent permission system」，agent 不问就调工具。ACP 的 agent 会请求权限，不给 UI 就会卡住；从安全上也不允许。

形态：**复用 Delta 的行内评论卡骨架**，插在触发它的工具卡下方。

```
┌─ 1px 描边圆角 ───────────────────────────────┐
│ ● Codex 请求执行命令                          │
│   rm -rf .next && pnpm build                  │
│   工作目录: .delta/worktrees/auth             │
│   [本次允许] [本会话允许] [本 thread 一律允许] [拒绝] │
└──────────────────────────────────────────────┘
```

规则：

- 默认拒绝；不自动消失、不自动拒绝、不超时。
- 结果写一行事件：`Allowed for this session` / `Denied`。
- 来自子 agent 的请求显示来源链；`cwd` 不同时不继承「一律允许」。
- 多个待审批在收件箱按 thread 分组，thread 顶栏显示 `1 等待审批`。

---

## 10. 视觉规范

**可用证据**（Delta 深色截图，像素目测）：

| 角色 | 值 |
| --- | --- |
| 窗口底 | `#1B1D1F` |
| 面板/输入底 | `#222527` |
| 选中行 | `#2B2F32` |
| 主文字 | `#E4E4E4` |
| 次文字 | `#8B8F94` |
| 强调 | 蓝紫（链接、提及） |
| diff 新增/删除 | 低饱和绿底/红底 |
| 在线点 / 未读点 | 绿 / 蓝 |
| thread 色板 | 8 个圆点：orange / pink / lime / purple / yellow / coral / blue / gray |

- 字体三套独立：UI、Prose、Code；各自字号；ligatures 开关。
- 图标单色线性，1.5 px；kind 靠 glyph 区分。
- agent 头像用品牌图形或字母；人类头像圆形，标题栏重叠排列 20–24 px。
- 行高：列表 34–36 px，标题栏 44–52 px（目测）。
- 动效、圆角、间距 token：**没有证据，不出规格**，交给视觉稿。

**未验证**：浅色主题的产品截图、流式光标、协作者光标色板、面板像素宽度。

---

## 11. 键位（采用子集）

| 动作 | macOS | Linux/Windows |
| --- | --- | --- |
| 发送 | `⌘Enter` | `Ctrl+Enter` |
| 命令面板 | `⌘⇧P` | `Ctrl+Shift+P` |
| 命令 + thread 搜索 | `⌘K` | `Ctrl+K` |
| 找 thread（含归档） | `⌘T` | `Ctrl+T` |
| 文件查找 | `⌘P` | `Ctrl+P` |
| 上一个/下一个 thread | `Ctrl+Tab` / `Ctrl+⇧+Tab` | 同 |
| 关右栏 | `⇧Escape` | `⇧Escape` |
| Review Changes | `⌥⇧D` | `Alt+Shift+D` |
| 子 thread 总览 | `⌘J` | `Ctrl+J` |
| tab 升主视图 | `⌘⌥Enter` | `Ctrl+Alt+Enter` |
| 回父 thread | `⌘⌥U` | `Ctrl+Alt+U` |
| Share | `⌘⇧S` | `Ctrl+Shift+S` |
| 复制选区链接 | `⌘⌥L` | `Ctrl+Alt+L` |
| 评论选区 | `⌘⌥M` | `Ctrl+Alt+M` |
| 重开已投递评论 | `⌘⇧M` | `Ctrl+Shift+M` |
| 归档 thread | `⌘W` | `Ctrl+W` |
| 标为已读/未读 | `⌘⇧U` | `Ctrl+Shift+U` |
| 跟随下一位 | `Ctrl+⌥⌘F` | `Ctrl+Alt+Shift+F` |
| Inbox | `⌘⇧J` | `Ctrl+Shift+J` |
| 文件 Seen | `⌘⌥Y` | `Ctrl+Alt+Y` |
| 全部 Seen | `⌘⇧Y` | `Ctrl+Shift+Y` |
| 项目/分支选择 | `⌘⌥O` / `⌘⌃B` | `Ctrl+Alt+O` / `Ctrl+Alt+B` |

Delta 自身存在键位冲突（`⌘⇧Enter` 是 fork 还是 subthread、`⌘⌥C` 是复制还是评论模式、`⌘⇧L` 既是日志又是 merge）。我们每个动作只保留一个键位，按上表为准，不复制冲突。

---

## 12. 与 Delta 的差异清单

| # | 差异 | 理由 | 影响 |
| --- | --- | --- | --- |
| 1 | 有审批 UI | Delta 无权限系统；ACP agent 会请求权限，无 UI 会卡死且有安全风险 | 新增第 9 节；其余形态照 Delta |
| 2 | project 为一级维度 | 产品定义要求「以 project 为维度」 | 左栏分组改为 Projects 常驻 |
| 3 | 子 thread 无 composer | ACP v1 RFD 禁止向子会话发 prompt | 提供「让父 agent 转达」模板 |
| 4 | 无 Reviews / Merge 两类 thread | 不自建 harness，review/merge 流程放到 P2 | 标题栏少两个动作 |

---

## 13. 状态矩阵

| 状态 | 表现 |
| --- | --- |
| 空：project 无 thread | 灰字 `No threads yet` + `New Thread` 行 |
| 空：thread 无变更 | 时间线一行 `No file changes yet.` |
| 空：无子 agent | 树不显示子分组，不显示占位 |
| 加载：worktree 准备 | 状态栏 `Preparing…`，tooltip 显示步骤 |
| 加载：子 thread 未就绪 | tab 显示骨架，不显示空对话 |
| 错误：路径失效 | 左栏警告 + `Locate…` |
| 错误：agent 崩溃 | 节点红点 + 停止原因 + `Restart` |
| 错误：thread 分叉 | 底部 footer `This thread has diverged` + `Keep Local Changes in a New Thread` / `Discard` |
| 权限不足 | 只读视图 + `Request access`，不显示禁用到底的输入框 |
| 断线 | 顶部条 `Reconnecting…`，输入保留，重连后按 `seq` 补齐 |
| 等待审批 | 顶栏 `1 等待审批`，审批卡常驻，不自动消失 |

---

## 14. 不做承诺的项

以下 Delta 行为没有可靠证据，v1 不实现、不写进验收：

- 工具卡上的单次耗时与 token。
- 流式光标与逐字动画的具体样式。
- 协作者光标的色板与头像边框色。
- 面板像素宽度、间距 token、圆角、动效时长。
- 浅色主题的具体配色（只保证可切换）。
- Reviews / Merge thread 的完整流程。
