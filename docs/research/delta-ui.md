# Delta UI / 交互调研（用于 Threadbase UI 规格）

> 标注约定：`[实]` = 直接来自官方文档原文或我亲自查看过的截图；`[推]` = 我的推断（文档/截图未直接这样说）；`[未验证]` = 无法确认，禁止当结论使用。
> 英文 UI 字符串一律保留原文，不翻译、不改写。

---

## 1. 调研范围与可信度

- **对象**：Delta（Zed Industries），桌面 app **v0.24.0（2026-11-04 核对）** [实]，Delta Web（delta.dev/threads），Delta CLI / Claude Code 插件（Beta）。
- **资料落盘时间**：2026-02（`_raw/` 抓取），原文来自 delta.dev 公开页面。
- **来源分层**：
  1. **一手规格**：`/docs/delta-101.md`（官方自称 “release-scoped product reference”，交代了版本与出处）+ 各功能文档页。**所有交互细节以这一层为准**。
  2. **一手视觉**：官网/博客引用并已下载的截图（53 张）。我只对**自己 read 过**的图写布局结论，逐条给出图片原始 URL。
  3. **佐证**：博客（The Diff 月报、planning、multiplayer）。只用来证明"功能存在"，不作交互规格。
  4. **营销页**（首页/Pricing/Download）：只证明定位与计划档位。
- **已知证据缺口**：`zed-industries/delta-claude` 的 GitHub README **没有取到**（抓取时 GitHub API 限流），插件行为以 `/docs/cli/claude-code.md` 为准。演示视频（Cloudflare Stream HLS、YouTube）**均未观看**，一律标 `[未验证]`。
- **时效风险**：Delta 处于 public beta，两个 The Diff 月报之间（2026-09 → 2026-10）就出现了侧栏配色数量、标签模型、嵌套子 thread 等变更。**任何 UI 细节都可能在下个版本变**，本文以 v0.24.0 为准。

---

## 2. 信息架构（文本树）

这是把文档 + 截图拼出来的**桌面端**结构 `[实/推混合，逐项标注]`：

```
App 窗口
├─ 标题栏（跨全宽，单行）                                     [实：截图 15/31/37/39/45]
│  ├ 左：系统红绿灯 · 侧栏开关 · 全局搜索(⌘F? → 实为搜索图标)
│  ├ 中：(侧栏右侧) thread 标题 + 参与者头像（1~3 个圆头像，重叠排列）
│  └ 右：Reviews(眼睛) · Merge(分支) · Share(带文字) · 右侧栏开关
├─ 左栏 Sidebar（可折叠 Toggle Sidebar）
│  ├ 组织名前缀行（如 "Zed"）→ Overview                    [实：截图 15]
│  ├ Inbox（桌面在标题栏托盘图标；Web 在左栏顶部）
│  ├ 分组：Pinned
│  │   └ thread 行（chevron + 标题 + 色条 + 子行缩进 + 右侧头像/未读点 + "Show 7 more"）
│  ├ 分组：Projects
│  │   └ project 行（folder 图标 + 名称，可右键 Rename Project）
│  │        ├ New Thread 动作行
│  │        └ thread 行 → 子 thread → 孙 thread（任意深度，带引导线）
│  ├ 分组：Threads（无 project 的 thread）/ Web 端：Shared with You
│  ├ 视图：Bookmarks（标题栏书签按钮）、Thread History（时钟按钮）
│  └ 底部：头像 · 名字 · 组织名 · 在线绿点 → 用户菜单（Settings/Keymap/Send Feedback/Sign Out/Organization 切换）
├─ 主区 Thread
│  ├ 时间线（单列，居中可配；左侧头像栏 ~56px）
│  │   ├ 用户消息（头像 + 正文）
│  │   ├ 他人草稿（按作者方块展示，可见"正在输入"的内容）      [实：collaborate-thread]
│  │   ├ agent 消息（字母/品牌图标头像 + 正文）
│  │   ├ 折叠摘要行：「Ran 2 commands, thought, read a file ›」
│  │   ├ Thinking › / Context Compacted › / Compaction failed
│  │   ├ 工具卡（Apply Patch / Read File 头部 + 文件路径 + diff）
│  │   ├ 终端块（"zsh ⌄" + 终端内容）
│  │   ├ 行内评论卡（引用范围高亮 + 头像 + 回复堆叠）
│  │   ├ 事件行（External changes: new commit … / Routed to Subthread / Sent·Received Agent Message / Merged|Merge Failed 卡 / Submit Review verdict）
│  │   └ 右缘导航轨（每个 turn 一个刻度，⌘+click 书签，Option+滚轮逐 turn 跳）
│  ├ Composer（底部，与时间线同宽）
│  └ Tab 条（次 thread / 文件 / 子 thread 以 tab 打开时出现）
├─ 右栏（可开关，Shift+Esc 关）
│  ├ 文件面板（agent 动过的 / 你打开的）
│  ├ 项目面板（worktree 文件树）
│  └ Changes（diff 审阅）
└─ 浮层/独立窗
   ├ Settings（独立窗口：左分类 + 右页，⌘,）
   ├ Command Palette（⌘K / ⌘⇧P）→ Tab 切 "Ask Delta"
   ├ View All Subthreads（⌘J）
   ├ Inbox（⌘⇧J）
   ├ Share popover（⌘⇧S）
   ├ Project Settings popover（⌘⌥O，含 Work In / 分支 / Check）
   ├ Terminals picker（@!）、@ picker、模型选择器、Send 按钮右键菜单
   └ Thread 右键菜单（颜色点 / Tags… / Archive / Delete …）
```

**一级维度差异（对我们重要）**：Delta 的左栏一级维度是 **organization → (Pinned / Projects / Threads)**，project 是其中一个分组；而 Threadbase 的 project 是一级维度。`[实]` 见 `docs/agents/threads.md` 的 "Group by: Project | Time | State | Owner | Tags"。也就是说 Delta 把"project 分组"做成了**可选分组方式之一**，默认按 Project。`[实]`

---

## 3. 屏幕清单

### 3.1 应用外壳（桌面，默认三栏 / 两栏）

**布局** `[实：截图 03/15/31/37/39/45 一致]`
- 标题栏横跨整宽，**左栏与主区在标题栏下才分栏**；标题栏高度按截图估计 ~44–52px（1000px 宽截图里约 40px `[推：按像素目测，非官方数值]`）。
- 左栏固定宽，右栏可开可关。`[实]` 文档：**Toggle Right Sidebar**、**Toggle Project Panel** 两个独立开关；`Shift+Escape` 关右栏。
- 左栏可整列折叠（Toggle Sidebar，标题栏左侧的矩形图标）。`[实：截图 03/15 的图标 + keybindings "Toggle Sidebar"]`
- 主区时间线是**单列**，内容宽度三档：`default`（标准可读宽度）/ `full`（铺满）/ `custom`（拖动 thread 里的 line-width ruler，右键菜单 **Toggle Line Width Ruler**），另有 **Constrain Wide Content**、**Center Content** 两个开关。`[实：settings.md]`

**左栏层级与状态表达** `[实：截图 03/07/15/17/37/39/45 + threads.md]`
- 行结构：`chevron（有子级才显示） · 图标 · 标题 · （副标题=project 名）· 右侧指示（头像 / 未读点 / 书签）`。
- **状态表达方式**（这是最值得抄的部分）：
  - **运行中**：折叠的分组/父行上显示 running indicator（文档："a collapsed section shows a running indicator while a thread inside it is working"）`[实]`。
  - **未读**：行尾蓝色小圆点（截图 37/39 的 `The Diff (September 2026)` 行）`[实：目测，文档未命名该点]` `[推：为未读点]`。
  - **被标记**：标题颜色/字重更亮（截图 17 的 "Mobile design revamp — MAIN" 明显更亮更粗）`[实：目测]`。
  - **已归档**：从主列表消失，需 `Group by: State` 或 Thread History 才看得到。`[实]`
  - **颜色**：thread 左侧竖色条 = 该 thread 的第一个彩色标签（tag）；子 thread 颜色**不继承**父 thread（0.24 起）。`[实]`
  - **kind 图标**（截图 17 可辨）：眼睛 = Review thread；分支 = Merge thread；对话气泡 = 普通子 thread/subagent。`[实：图 17 + review-and-sync "eye button (Reviews)"]`
- **搜索与筛选** `[实]`
  - 过滤菜单 **Filter Threads**：`Filter by`（Projects / Owners / Subthreads / Tags）、`Group by`（Project / Time / State / Owner / Tags）、`Sort by`（Creation Time / Last Prompted / Last Response / Manual）、`Display`（Threads Per Project、Subthreads Per Thread：10 / 25 / 50 / All；Show Tag Names）。
  - 子 thread 分四类可分别开关：**Plain / Reviews / Merge / Subagents**。
  - 全局：`⌘K` 同时搜命令与 thread；`⌘T` 直接进 thread 搜索（含归档）；`⌘P` 文件查找，支持 `path:line:column`。

**空状态/加载/错误（外壳层）** `[实：截图 03/07/37/45]`
- 项目下没有 thread：项目行下方灰字 **No threads yet**（截图 03，`delta-meta`）。`[实：目测文案]`
- thread 无文件变更：时间线里出现一行 **No file changes yet.**（截图 45）。`[实：目测文案]` `[推：应为变更摘要事件的空态]`
- worktree 准备中：状态栏控制显示 **Preparing…**，tooltip 显示当前步骤；失败显示错误 + **Retry**。`[实]`
- 项目文件夹被移动/删除：左栏该项目打警告标记，点击可 **Locate…**。`[实]`
- thread 与服务端分叉：底部出现 footer **This thread has diverged from the server**，两个动作 **Keep Local Changes in a New Thread** / **Discard Local Changes**（更早期有 **Fork & Restore** / **Discard Changes**）。`[实]`

### 3.2 Thread 视图（主区）

**头部固定展示什么** `[实：截图 31 最完整]`
- 标题栏里：`thread 标题` + `参与者头像`（重叠小圆，>1 人时并列）。
- 右侧 cluster，从左到右：**眼睛（Reviews）→ 分支（Merge）→ Share（带文字的按钮）→ 右侧栏开关**。
- 分支/worktree/模型**不在头部**：分支在**底部状态栏**（截图 03/07/31/33：左下角 `delta  main` + 一个"+"文件夹图标，即 Project Settings 控制）。`[实：目测 + delta-101 "The status bar's Project Settings control"]`
- 移动端/窄屏头部是另一套：标题 + 副标题 `delta / main`（project / 分支）+ 右侧 inbox 与 `…`。`[实：截图 21]`

**时间线组成与顺序** `[实：文档 + 截图 14/31/37/39/45]`
1. 用户消息（头像在左；人的头像 = 圆形照片）。
2. 被发送时合并进来的草稿：多人连续发言时**每条都带自己的头像**，不合并成一段（截图 37 两条相邻问题来自两个不同人）。`[实：目测 + "sending submits all trailing drafts under their respective authors"]`
3. agent 输出段落（头像为品牌/字母图标，如 `A\`、OpenAI 图形）。
4. **折叠摘要行**：单行灰字 + 行尾 `›`，例如 `Called 2 tools, thought, fetched a URL ›`、`Ran 5 commands, thought twice, read a file ›`、`Loaded 2 skills, read a file, called a tool, thought ›`。`[实：截图 14/21/31/37/39/45 的大量样本]`
   - 组合顺序就是动作发生顺序；**这是 Delta 时间线最主要的密度控制器**。
5. 特殊折叠行：`Thinking ›`（截图 37）、`2 comments ›`（截图 45）、`Context Compacted ›`（截图 33）。
6. 工具卡：头部为动作名（`Apply patch` / `Read File` / `Edit File`），下一行文件路径，右侧 chevron，card 内是带语法高亮的 diff（新增行绿底、删除行红底）；`[实：截图 21 的 Apply patch 卡]`
7. 终端块：`zsh ⌄`（可切程序）+ 圆角描边终端内容 + 左上角与消息同列的头像。`[实：截图 16]`
8. 行内评论卡（引用范围在正文里高亮 + 卡片内头像与回复堆叠）。`[实：截图 37/39/41/45/31]`
9. 事件行：`External changes: new commit bba7929 in cloud ›`、`Routed to Subthread`、`Sent Agent Message` / `Received Agent Message`、`Merged` / `Merge Failed` 结果卡（含 target branch、commit、CI 状态）。`[实]`
10. **没有审批请求行**：Delta 明确"没有 agent 权限系统，agent 不问就调工具" `[实：agentic-safety.md]`。→ 见第 6 节，这是我们必须自研的空白区。

**耗时/token 的展示** `[实：截图 21 底部控件 + 文档]`
- 上下文用量是底部**圆形指示器**，点开是 **Context Window** popover（`Automatic Compaction`、`Compact at`、`View Context Breakdown`）。`[实]`
- 单个工具卡的耗时/token 数字：**没有在任何截图里看到** `[未验证]`。文档只说明会折叠"low-value tool output"，不承诺计时。**不要编造耗时 UI。**

**流式输出的视觉处理** `[推/未验证]`
- 文档说 comment 回复会"在 agent 仍在流式输出时出现在其响应旁边" `[实]`。截图里看不到进行中态（都是已完成）。流式光标/骨架样式 **`[未验证]`**。
- 有 fade：截图 45 顶部与底部滚动边缘明显有渐隐遮罩 `[实：目测]` `[推：滚动边缘 mask-gradient]`。

**输入框（composer）** `[实：截图 21/31/33 + threads.md + settings.md]`
- 底部一行控件（移动端截图 21 最清楚）：`上下文圆环` · `（一个双箭头图标，语义未标注）` · `💡 Medium`（thinking effort）· `模型名 6.1 Sol` · `显示器图标`（runner：Local / Cloud）。桌面截图 33 底部同构：`⌘ delta main` · 圆环 · `💡 Max` · `A\ Fable 5.1` · `🖥` · `➤`。`[实：截图 21/33 目测]`
- 发送：默认 **`⌘Enter` 发送，`Enter` 换行**；关掉 **Send With Modifier** 后 `Enter` 发送。发送按钮右键也能切。`[实]`
- **发送/插话语义**：生成中再发一条消息 = **立刻停止当前响应并发送新消息**；**Cancel Generation** 只停止不发送。**没有排队（queue）概念**。`[实：delta-101 "Sending another message during generation stops the current response and immediately sends the new message."]`
- 附件：图片可直接粘进 thread / 文件 / diff 的评论里，内联显示；文档未描述"附件按钮"，命令/文件用 `@`。`[实]`
- `@` picker 四类：**Files & Directories / Threads / Users / Terminals**，可用 `@file:` `@thread:` `@user:` `@terminal:` 或任意唯一缩写（`@th:`）。默认搜人与 thread；匹配类目在搜索时置顶。`[实]`
- `!` 行首开内联终端；`@!` 开 **Terminals** picker（`New Terminal in <project>` / 已有会话按运行程序名列出）。`[实：截图 16]`
- `:` 触发 emoji/数学符号补全；`Tab`/`Shift+Tab` 在正文里按两列缩进。
- 换行/发送/打断三件事都绑得很紧，**没有"排队到下一轮"的按钮**。`[实]`

### 3.3 子 agent / 子 thread（Delta 用「树」，不是卡片）

**结论：树 + 可开 tab + 可开成主视图，没有"嵌套卡片"。** `[实：截图 17 + subagents.md + threads.md]`

- **入口**
  1. 左栏父行下**缩进嵌套**（任意深度，0.24 起支持 subthread 里再 `/fork`）。
  2. `Cmd+J` / **View All Subthreads**：模糊查找器，只列**一层**直接子级，`⌘1–⌘5` 过滤 all/reviews/merges/plain/subagents，`Tab` 显示或隐藏归档。`[实]`
  3. agent 自己派的 = 普通 subthread，结束时把最终结果回给直接父级。`[实]`
- **缩进与引导线**：每层缩进 + 一条极细竖线（截图 17 可数到 4 层：`Delta Web Mobile UI → Delta Web - Mobile UI Proposal → Mobile design revamp — MAIN → Land mobile mock UI changes`）。`[实：截图 17]`
- **状态**：与普通 thread 相同（running indicator、未读点、颜色标签可独立于父级）。`[实]`
- **打开方式**：点击 = 开成主视图；`⌘`+点击 = 开成 tab；行 hover 有 **Open in Tab** 按钮；tab 内 `⌘⌥Enter` = **Open as Primary**，`⌘⌥U` = 回父级。`[实]`
- **单独看某个子 agent 的 transcript**：可以，它就是一条普通 thread，有自己的 transcript、自己的 composer，可以**你直接接管发消息**驱动它。`[实]`
- **派遣时刻在主时间线的呈现** `[实]`
  - 用户在父 thread 发出、被路由进子 thread 的那条消息：留在父时间线里**变成只读**，带 **subthread badge** 和一行 **Route to subthread**，点名字在 tab 打开（tab 显示 loading 直到 ready）。
  - 子 thread 事件行带**按 kind 上色的 badge**（review / merge / 其它）。
  - agent 之间互发消息：**Sent Agent Message** / **Received Agent Message** 折叠事件，展开才看到内容；点发送方名字跳到对方 thread。
  - 用户自建的子 thread **不**向父级报进度，只有 **Merge Changes** 结果例外。
  - **没有"子 agent 卡片网格"这种形态** `[实：全部截图均未见]`。
- **停止**：聚焦某个在跑的子 agent 时按 `Escape` **只停这一个**（不波及父级与兄弟）。`[实]`

### 3.4 变更审阅（Changes）

**面板位置** `[实]`：右侧 drawer 的**默认面板**（"on a computer, it is the review drawer's default panel"）；手机上在标题下的 tab bar 里 (`Changes +694 −85`)。

**打开方式**：`File > Review Changes` 或 `⌥⇧D`；点底部状态栏的增删行计数；`⌘⇧R` 是**另开 review thread**（不同概念）。`[实]`

**比较基线选择器**（Changes selector）`[实]`
- `origin/main`（带 base 名的分支 diff）· **Last Commit**（未提交）· **Last Turn**（自上次 agent turn 开始）· 从 thread 里点开的某个 commit。
- 默认基线由 `Settings > Pane > Default Diff Base` 决定（`branch` / `uncommitted` / `last_turn`）。
- 多人时分支比较可能来自**别人的 checkout**：选择器会注明来源；分支选择器分三组 **Your Checkout** / 协作者 checkout（带其头像）/ **Another Checkout**；旁边有 **Fetch Diff** 按钮。`[实]`

**文件树/列表** `[实]`
- 每个文件标 added / modified / deleted / renamed；重命名会显示旧路径，diff header 高亮路径变化。
- 每行一个 **Seen** 复选框；勾选后折叠该文件，文件再次变更则自动清除。
- header 显示进度 `3/17`。
- `Display Options`：**Collapse All Files / Expand All Files**（`⇧Tab`）、**Mark All as Seen / Mark All as Unseen**（`⌘⇧Y`）。
- 文件间移动：`⌘[` / `⌘]`。
- 长列表有可拖拽滚动条。

**hunk / 行操作** `[实：截图 19 的 Changes 视图]`
- 行号栏左侧有 `↑` `↓` 两个按钮 = 上一个/下一个 hunk。
- 有行号、语法高亮；选中整行可拖行号（文件 tab 内）；搜索 `⌘F` 跨整个比较的所有文件。

**行内评论** `[实]`
- 在 file tab / diff 里**选中代码直接打字 = 建评论**（comment mode 默认开）；或右键 **Comment**；`⌘⌥M`。
- 文件工具栏的**气泡开关**切换 **Comment Mode On / Off**（实心=评论，空心=编辑文件）。
- 交付后的评论收进 gutter 指示器，`⌘⇧M` 重新打开；`⌘⌥V` 在 Files/Changes 里显隐评论。
- 评论是 **pending**，直到下一次发送；也可单独提交。

**keep / undo** `[实]`
- **没有 per-hunk 的 keep/undo 按钮**（全部截图 + 全部文档均无此 UI）。
- 替代机制：①`Edit > Revert Conversation to Cursor` 把对话与文件**一起**回滚到光标处；②改基线比较（Before Last Turn 只读视图）看"上一轮之前长什么样"；③`Pull Changes`（从 review verdict 拉回）；④`/merge-into-parent`、`/pull-from-subthread`；⑤push 到 `local` remote。

**Review & Sync 具体流程与按钮文案** `[实]`
1. 头部 **眼睛（Reviews）** → 菜单（列表里是已有 review + **Start Review**；归档 review 不在菜单里）。
2. 也可以在 composer 打 `/review`，或 `⌘⇧R`。
3. review thread 里 agent 先产出 **change guide**（按"讲得通"的顺序组织，而不是按文件）；标题自动生成期间显示 **Generating title…**。
4. 结束时 **Submit Review** → **Approve** / **Request Changes**（也可 `/approve`、`/request-changes`）。
5. verdict 回到父 thread；父 thread 头部该 reviewer 的头像上加 **approved / changes-requested 徽标**。
6. verdict 上有 **Pull Changes**；拉取成功后按钮变 **Changes Pulled** 且禁用。
7. review thread 自己**没有分享设置**，跟随父级；父级没分享时 **Copy Share Link 是 disable**。

**与 worktree/checkout 的关系在 UI 上的体现** `[实]`
- **Work In** 弹层（Project Settings，`⌘⌥O`）：`Isolated Delta Worktree (Default)` / `Existing Local Checkout` / `Existing Git Worktree ›`（子菜单带 "Search Git worktrees…" 搜索框 + 每个 worktree 一行：分支图标、名字、`Detached @ 95cd535 · …rktrees/zed/gilded-otter/zed`）。`[实：截图 07 完整可见]`
- 同一弹层还放：分支行（`main ⌄` + `⌘⇧B` + `Conrad Irwin · 95cd535 · 2 hours ago`）、远端状态行（绿点 `Up to date with remote` + **Check** 按钮）、`View Files ⌘⇧E`、`Copy… ›`、`Remove Project`。`[实：截图 07]`
- 选择在**第一条消息发出后锁定**；多人复用同一 checkout 时会警告改冲突。`[实]`

### 3.5 协作

**Share 弹层** `[实：collaborate-thread.md]`
- 触发：右上 **share 图标** / `⌘⇧S` / `File > Share Thread` / 命令面板 / 左栏 thread 右键（菜单项文案 **Copy Share Link**）。
- 三档访问级别（原文照抄）：**Only people invited** · **Everyone in your organization**（文案里带组织名）· **Anyone with the link**。**只有 owner 能改**；所有人能看设置、成员列表、复制链接。
- 邀请：按邮箱或 `@username`，**一次最多 10 人**，输入 `@` 有联想，**邀请 14 天过期**；被邀请者是 **editor**。
- 复制链接**不关闭弹层**；私有 thread 在弹层里就地警告"这个链接不授权"。切到 **Only people invited** 不撤销已加入的人。
- 打开到 **Anyone with the link** 之前**先要确认**。
- **Copy Link to Selection** / **Copy Link to This Position**（`⌘⌥L`）：链接到选区/位置，打开后**目标高亮、周边变暗**，直到 `Escape` / **Exit Focus**。

**在场与跟随** `[实/部分未验证]`
- 参与者头像出现在**标题栏**（thread 标题右侧）与**协作者 checkout 的分支选择器**里。
- 跟随：点头像 → **Follow**；或 `Ctrl+Option+Cmd+F` = **Follow Next Collaborator**（循环）。跟随会跨越 thread 视图、进出子 thread（tab 或主视图）、打开对方看的图片、复制其缩放与平移。`[实]`
- **光标/选区的实时色块**：截图里能看到正文中的**评论范围高亮**（多种颜色）与正在输入的竖线光标（截图 37 最后一条评论里有 `|`），但**协作者实时光标颜色体系、头像边框色、选区色板均无文档/无清晰截图** → `[未验证]`。
- 通过外部工具（如 Claude Code）接入的协作者会在 **participant bar** 里带一个 **badge**，tooltip 写工具名。`[实]`（badge 具体样式 `[未验证]`）
- 状态：左栏底部有**绿点**（在线）`[实：截图 07 目测]`，语义文档未命名 `[推]`。

**评论 / @提及 / Inbox** `[实]`
- 评论：选中 agent 输出直接打字 = 建评论；回复堆叠；每条回复左侧有作者色竖线；卡片右上角有 ✓✓ 图标 `[实：截图 39/45 目测，语义未标注]` `[推：可能=已投递/已解决]`。
- `@` 提及：选人立刻进 Inbox（**发送前就通知**）；手打完整 `@username` 则等到发送才通知；被提及可**顺带把私有 thread 分享给对方**；撤回未发送的提及**不会**收回已发出的通知。
- Inbox：`⌘⇧J`（Web 是 `Alt+Shift+J`），列表分 **Unread / All** 两个过滤，有 **Mark All As Read**；条目形态见截图 19：头像 + `@franciskafyi mentioned you in a thread` + 第二行 `6 hours ago · <thread 标题>`；标签写作 **Unread (0)**。
- 打开 thread 即把该 thread 的提及与邀请标记为已读。

### 3.6 Web 与桌面

**delta.dev/threads 能做什么** `[实：delta-on-the-web.md]`
- 浏览你有权访问的 thread（`My Threads` / `All Threads` / 按 Organization 切；`Previous` / `Next` 翻页；可加到家目录当 PWA）。
- 在浏览器里**发消息、评论、审 diff、提交 review verdict**、`View All Subthreads`（`⌘J`）、`Open as Primary`、向子 thread 发消息。
- 浏览器 turn **有**：读 thread 的 diffs、replicated 文件；**没有**：shell、checkout、你机器上的凭据/环境变量/进程。所以**不能跑终端命令**。
- 浏览器**不能**：建顶层 thread、管理 project、合并/skill 的 **Merge** 流程（"This workflow is not available in the browser"）、桌面快捷键的一部分（`⌘D` 等被浏览器占用）。
- 加入体验边界：**「不安装也能加入」= 打开链接 + 用 Zed 账号登录 + 接受 Early Access Agreement，直接进浏览器版完整 thread 视图**；需要原生能力时头部有 **Open in Desktop App**（其菜单里还有 **Download Delta**），手机上不显示该控件。
- 移动 Web 是专门布局：标题下 `Thread / Changes / Files` 三 tab；`More` 里放 Reviews / Merge / Search / Line Numbers / Comment Mode；`Message Overview` 底部抽屉；Inbox 底部抽屉。`[实：截图 19/20/21]`

---

## 4. 交互规则（键位 / 跟随 / 接管分享 / 评论 / 审阅）

### 4.1 键位（macOS / Linux·Windows）`[实：keybindings.md + delta-101，两处交叉核对]`

| 动作 | macOS | Linux/Win |
| --- | --- | --- |
| 发送消息 | `⌘Enter` | `Ctrl+Enter` |
| 发送到 fork/子 thread | `⌘⇧Enter` | `Ctrl+Shift+Enter` |
| 循环 thinking effort | `Ctrl+'` | `Ctrl+'` |
| 命令面板 | `⌘⇧P` | `Ctrl+Shift+P` |
| 命令+thread 搜索 | `⌘K` | `Ctrl+K` |
| Keymap 设置页 | `⌘/` | `Ctrl+/` |
| Settings | `⌘,` | `Ctrl+,` |
| 找 thread（含归档） | `⌘T` | `Ctrl+T` |
| 文件查找（支持 path:line） | `⌘P` | `Ctrl+P` |
| 上一个/下一个 thread | `Ctrl+Tab` / `Ctrl+Shift+Tab` | 同 |
| thread ↔ 文件面板焦点 | `⌘?` | `Ctrl+?` |
| 聚焦父对话 | `⌘1` | `Ctrl+1` |
| 右侧栏关闭 | `⇧Escape` | `⇧Escape` |
| Review Changes | `⌥⇧D` | `Alt+Shift+D` |
| 新建 review thread | `⌘⇧R` | `Ctrl+Shift+R` |
| Merge Changes | `⌘⇧L` | `Alt+L` |
| 子 thread 总览 | `⌘J` | `Ctrl+J` |
| tab 里的子 thread 升为主视图 | `⌘⌥Enter` | `Ctrl+Alt+Enter` |
| 回父 thread | `⌘⌥U` | `Ctrl+Alt+U` |
| Share | `⌘⇧S` | `Ctrl+Shift+S` |
| 复制分享/选区链接 | `⌘⌥L` | `Ctrl+Alt+L` |
| 评论选区 | `⌘⌥M` | `Ctrl+Alt+M` |
| 文件 tab 里切 comment mode | `⌘⌥C` | `Ctrl+Alt+C` |
| 重开已投递评论 | `⌘⇧M` | `Ctrl+Shift+M` |
| Files/Changes 显隐评论 | `⌘⌥V` | `Ctrl+Alt+V` |
| 归档 thread | `⌘W` | `Ctrl+W` |
| 重开最近归档 | `⌘⇧T` | `Ctrl+Shift+T` |
| 标为已读/未读 | `⌘⇧U` | `Ctrl+Shift+U` |
| 书签 thread | `⌘D` | `Ctrl+D` |
| 书签列表 | `⌘⌥B` | 无默认 |
| 跟随下一位协作者 | `Ctrl+⌥⌘F` | `Ctrl+Alt+Shift+F` |
| 建内联终端 | ``Ctrl+` `` | ``Ctrl+` `` |
| 终端转后台 | `⌘⌥Z` | `Ctrl+Alt+Z` |
| Add Project 菜单 | `⌘⇧A` | `Ctrl+Shift+A` |
| 项目/分支选择器 | `⌘⌥O` / `Ctrl+⌘B` | `Ctrl+Alt+O` / `Ctrl+Alt+B` |
| Inbox | `⌘⇧J` | `Ctrl+Shift+J` |
| 标记文件 Seen | `⌘⌥Y` | `Ctrl+Alt+Y` |
| 全部 Seen/Unseen | `⌘⇧Y` | `Ctrl+Shift+Y` |
| 折叠/展开全部 review 文件 | `⇧Tab` | `⇧Tab` |
| 文件间切换 | `⌘[` / `⌘]` | `Alt+[` / `Alt+]` |

- **不能自定义键位**："Delta does not support rebinding keys yet" `[实]`。
- 菜单栏每一项都显示对应快捷键；`Settings > Keymap` 是**从 live keymap 生成的**，所以永远和实际一致。`[实]`

### 4.2 跟随 / 接管

- **接管 = 没有"接管"按钮**。共享 thread 里**所有人都能发消息、steer、改文件、评论**，权限只有 owner/editor/viewer 的粗粒度区分；**模型用量记在"发送者"头上**，agent turn **在发送者所在环境跑**（桌面/浏览器/云）。`[实]`
- 所以"交接"是隐式的：打开同一个 thread 直接说话即可；agent 看到每条消息的作者。`[实]`
- **草稿即聊天**：草稿对所有参与者可见、可按作者区分、**但任何人不点发送，agent 就不会收到**。发送时**末尾所有作者的草稿一起提交**成一个 turn，记在发送者账上。文档明确"没有办法只发给协作者"。`[实]` ← 这条对 Threadbase 的 CRDT 草稿设计几乎是原样答案。

### 4.3 评论规则

- 锚点：正文 span / 文件行 / diff 行 / 评论本身（回复）。
- 生命周期：draft（本地）→ pending（未发送，随消息一起送）→ delivered（collapsed 到 gutter）。**发送会连协作者还停在里面的 pending 评论一起送出**。`[实]`
- 代码被删掉后再打开评论：显示**当时那一版**文件/diff，**View Latest** 回到当前。`[实]`
- agent 的回复**反向链接**到评论；评论摘要里点文件名跳该位置。`[实]`

### 4.4 审阅规则

- 一个 thread 可以有多个 review；review 挂在父级下、可单独归档。
- verdict 可多次提交，**只保留当前 verdict**，历史留在父 thread 历史里。
- review 内 agent 的改动在**独立的 forked worktree**，不影响父级文件；要拿回来必须 **Pull Changes**（按 verdict 快照）。
- Merge 结果**没有** Pull Changes，只有 **Merge into Parent** / 让 agent 合并。

---

## 5. 视觉规范观察

> 只能写我**亲眼看过**的：截图 03、07、14、15、16、17、18、19、21、31、33、37、39、41、45、05、02、26、00。所有数值为**按像素目测的比例**，不是 design token。

**主题** `[实]`
- 深色是主推（所有产品截图均为深色）。浅色存在：`Settings > Appearance` 可 `follow system` / 强制 light / dark，菜单栏也有 **Toggle Light/Dark Theme**。`[实]`
- **浅色 UI 截图我一张也没有** → 浅色配色 `[未验证]`。品牌侧的奶油底 `#F3F0E9` 左右出现在 OG 卡（图 00）与博客示意图（图 26）`[实：目测]`。

**配色角色（深色）** `[实：目测]`
- 背景三档：窗口底 ≈ `#1B1D1F`；面板/输入底略亮 ≈ `#222527`；选中行 ≈ `#2B2F32`。
- 三级文字：主 ≈ `#E4E4E4`；次（摘要行、时间戳、路径）≈ `#8B8F94`；禁用更暗。
- 强调色：链接/提及为**蓝紫色系**（截图 37/45 的 `@franciskafyi`、下划线链接）。
- diff：绿底/红底低饱和半透明；另有 **`blue_orange`** 色盲友好方案与 **Diff Markers**（`+` / `−` 符号，默认关）。`[实：settings.md]`
- **线程颜色 = 标签颜色**，0.24 的色板是 8 个圆点：orange / pink / lime / purple / yellow / coral / blue / gray（截图 15）。9 月版本只有 5 个 + 一个 `×` 清除点（截图 03）。`[实：两图对比]` → 说明色板本身在演进。
- 在线状态点：绿。未读点：蓝。

**栅格与密度** `[实：目测 + 文档]`
- 三栏比例（以截图 15、宽度约 1180px 的窗口段估算）：左栏 ≈ 38%，主区余下；截图 03 里左栏 ≈ 635/1690 可用宽度。**官方数值 `[未验证]`**。左栏看起来是固定宽、非拖拽（文档没有 sidebar resize 的描述）。
- 时间线：头像独立左栏（约 56px），正文列有最大宽度（`default` 可读宽度）；可切 `full` / `custom`。
- 行高：列表行 ≈ 34–36px（截图 15 的 thread 行）；标题栏 ≈ 44–52px。
- prose 段间距明显大于行距（约 1em 段间距），标题层级靠**字重**而非字号剧烈变化（`Status` / `Settled` 这类 h3 与正文差距很小）。

**字体层级** `[实：settings.md + 目测]`
- 三套字体族可独立设置：**UI**（按钮/菜单/侧栏）、**Prose**（对话与渲染 Markdown）、**Code**（源码/diff/终端），各有独立 px 字号，另有 **Font Ligatures** 开关。`[实]`
- 品牌排版：OG 卡用**衬线大标题** + 衬线副文（图 00），与产品内部的 sans UI 不是一套。`[实：图 00]`

**图标风格** `[实：目测]`
- 单色线性图标（1.5px 左右），无填充块；kind 区分靠不同 glyph（眼睛/分支/气泡/文件夹/书签/时钟/托盘）。
- agent 头像：品牌图形（OpenAI 花形）或**字母头像**（如 `A\`、`A|`）——说明"每个 agent 身份有头像位"，可放品牌 logo。`[实：图 14/21/31/33/37]`
- 人类头像：圆形照片，标题栏里重叠排列，直径约 20–24px。`[实：目测]`

**动效** `[未验证]`
- 静态证据只有：滚动边缘渐隐（图 45）、"选中提及人时名字旁短暂出现铃铛"（release notes 文字描述）、"复制分享链接时弹层不关闭"。**转场时长、曲线、hover 动效均无法确认**。

---

## 6. 可借鉴清单（对 Threadbase 的直接建议）

### P0 —— 不给就会明显"不像 Delta"
1. **三栏外壳 + 标题栏承载 thread 身份**：标题栏左侧放侧栏开关与搜索；中间放 `thread 标题 + 参与者头像组`；右侧放 `Reviews / Merge / Share / 右栏开关` 四个动作。分支与 worktree 放**底部状态栏**（`project  branch` + 下拉），不要塞进头部。依据：截图 31/33 与 delta-101 的 "status bar's Project Settings control"。
2. **时间线密度靠"折叠摘要行"控制**：把连续的 read/search/command/tool 合并成一行 `Ran 3 commands, thought, read a file ›`，行尾 chevron 展开；文本段落永远不被折叠（除非开 `Collapse All Tools`）。文案模板直接照抄 Delta 的动词短语（`Ran N commands`、`thought`、`read a file`、`Called N tools`、`fetched a URL`、`loaded a skill`）。这是 Delta 观感的第一来源。
3. **左栏 project → thread 树**：project 作为可折叠父行，thread 缩进其下，子 thread 再缩进（任意深度），**用细竖引导线 + 每层 +1 缩进**；行右侧放头像/未读点/书签；每个 group 末尾 `Show N more`（默认 10）。状态用"点 + running indicator + 色条"三件套，不要用文字标签。
4. **子 agent 用树 + tab，不要卡片**：点击开主视图、`⌘`+点击开 tab、`⌘⌥Enter` 升为主视图、`⌘⌥U` 回父级、`⌘J` 一层内的模糊查找器。子 agent 自己就是一条有 composer 的 thread（可直接接管），不是只读面板。依据：subagents.md 明确"runs as a regular subthread with its own transcript and status"。
5. **Composer 的三段语义**：`⌘Enter` 发送 / `Enter` 换行；生成中发送 = **打断并立即发送**（无排队）；`Cancel Generation` 只停不发。底部一行控件按 Delta 顺序：`上下文圆环 · effort · 模型 · runner · 发送`。
6. **评论 = 选中即打字**：正文 span、文件行、diff 行三处同一手势；draft → pending（随消息发送）→ delivered（收进 gutter，`⌘⇧M` 重开）；draft 状态下协作者可见。这是 Delta 的核心差异点，我们的 `Comment` 领域对象应该按这个状态机建。
7. **⚠ 审批 UI 必须自研（Delta 没有）**：`agentic-safety.md` 明确"Delta does not have an agent permission system"，agent 不问就调工具。而 Threadbase 的 F4 要求 PermissionRequest 卡片。**建议**：复用 Delta 的**行内评论卡**形态（圆角 1px 描边、左侧作者色竖线、头像 + 一行说明）作为审批卡骨架，插在触发它的工具卡**下方同一位置**，并把结果写成一行事件（`Allowed for this session` / `Denied`）。这是"对齐 Delta"唯一必须破例的地方，要在需求文档里显式记录。

### P1 —— 做完 P0 后能明显提升
8. **Changes 选择器四档**：`origin/main` / `Last Commit` / `Last Turn` / `某次 commit`，默认档可配；每文件 `Seen` 复选框 + header `3/17` 进度 + `Mark All as Seen`；`⌘[ ⌘]` 跳文件、`⇧Tab` 全折叠。Delta 用"Seen"替代"已评审"，比逐 hunk keep 更省事。
9. **草稿即聊天 + 一起发送**：多人草稿各自可见、可编辑他人草稿、发送时把末尾所有作者的草稿合成一个 turn、用量记在发送者。直接对应我们"CRDT 只做共享草稿"的决策。
10. **@ picker 四类 + 前缀**：`Files & Directories / Threads / Users / Terminals`，支持 `@file:` `@thread:` `@user:` `@terminal:` 与唯一缩写；匹配类目置顶。
11. **Share 弹层三档 + 10 人邀请 + 14 天过期 + 复制不关弹层 + 私有链接就地警告**。这些细节几乎可以照搬文案结构（换成我们的措辞）。
12. **Copy Link to Selection**：选中任意 span/行，`⌘⌥L` 出链接，打开后目标高亮、周边变暗，`Escape` 退出。协作产品里性价比极高。
13. **Inbox 作为独立通知面**：`Unread / All` 两档 + `Unread (0)` 计数 + 每条 = 头像 + "谁 @ 了你" + 相对时间 · thread 标题；点开即清该 thread 的未读。
14. **颜色即标签**：不要同时做"颜色"和"标签"两套（Delta 自己承认这是两次尝试最后合并的）。8 色圆点 + 首个彩色标签决定 thread 颜色 + 右键色点行 + `Tags…` 子菜单。

### P2 —— 加分，但可以先不做
15. **Follow Next Collaborator**（`Ctrl+⌥⌘F`）跨 thread/子 thread/图片缩放跟随。
16. **导航轨**：时间线右缘按 turn 打刻度，`⌘`+点击书签某个 turn，`Option`+滚轮逐 turn 跳。
17. **Thread History / Bookmarks 两个独立视图**（书签可跨归档保留）。
18. **Settings 独立窗口**：左列可搜索分类（General / Appearance / LLM Providers / Thread / Subagents / Pane / Keymap / MCP / Skills…），底部 `Share Feedback` `View Docs`；`Copy Page Link` 产出 `delta://settings/…` 深链，在 thread 里点击可跳到该设置并高亮。
19. **命令面板里的 Ask Delta**：`Tab` 切自然语言 → 给命令建议。
20. **Merge / Review 的 kind 化**：把"评审""合并"都做成 thread 的类型（有自己的图标、badge、结果卡），而不是弹窗流程。**但这与我们"不自建 runtime、只做产品层"的定位有冲突，需要单独评估。**

---

## 7. 未验证与未知项

**明确没有证据的（不要写进规格）**
- **面板像素宽度、间距 token、字号 token、圆角半径、动效时长/曲线**：全部 `[未验证]`。截图多为裁剪图，无法换算真实 pt。
- **浅色主题的产品截图**：无。只有奶油底示意图与 OG 卡。
- **审批/权限 UI**：Delta 明确**没有**（不代表它会一直不做，roadmap 里有 "agent permissions"）。
- **单次工具调用的耗时/token 数字展示**：任何截图均未见。
- **流式输出中的视觉处理**（光标、骨架、逐字动画）：`[未验证]`。
- **在场光标/选区颜色体系、头像边框色、follow 时的视角提示**：`[未验证]`；只在评论范围高亮里看到多色。
- **Share 弹层、桌面 Inbox、桌面 Changes 文件列表、Keymap 页、Welcome 屏**：**一张截图都没有**，只有文字描述。
- **演示视频**：`the-diff-2026-10` 里的 Cloudflare Stream HLS manifest（`baf9bd237710dc1cefd3ae7415227ff7`）与两条 YouTube（`i1IProLb_UM`、`Y4mpyFDb4xc`）**未观看** `[未验证]`，不给时间点。
- **`zed-industries/delta-claude` 的 README**：抓取时 GitHub API 限流，**未取得**。插件文案以 `/docs/cli/claude-code.md` 为准（`/delta share`、`/delta join <link>`、`/delta setup`；join 时会问 "trust the new folder" → **Yes, move here**）。
- **移动端 composer 的第二个图标**（截图 21 里圆环右侧的"双箭头"）：语义不明 `[未验证]`。

**记录在案的矛盾（不要挑一个当真）**
1. `⌘⇧Enter`：`keybindings.md` 说 "Send a message to a fork"；`delta-101.md` 说 "sends the message to a **subthread**"；`subagents.md` 又说 "Every subthread an agent creates is a subagent"。→ **fork / subthread / subagent 三个词在 v0.24 里仍是同一套机制的三种叫法**，UI 上按 kind 分 Plain / Reviews / Merge / Subagents 四类。
2. `⌘⌥C`：`delta-101.md` = **Copy Thread Content**；`keybindings.md` = **toggle comment mode in a file tab**。
3. 日志查看器：`delta-101.md` = `⌘⇧L`；`keybindings.md` = `⌘⌥⇧L`。（且 `⌘⇧L` 同时也是 **Merge Changes**。）
4. 复制分享链接：文档 = `⌘⌥L`；但**截图 03 与 15 的右键菜单**显示 **Copy Share Link ⌘⇧S**（与 "Share Thread" 同键）。
5. 默认工作区名称：`core-concepts.md` 说 Delta 把默认方案标为 **Isolated Workspace**；其余文档与截图 07 都是 **Isolated Delta Worktree (Default)**。
6. 颜色点数：截图 03（9 月）5 色 + `×`；截图 15（10 月）8 色、无 `×`。
7. 左栏 `New Thread` 动作行的位置：截图 07 在 project 的第一行；截图 37 在 `delta-meta` 的**末尾**。`[未验证]` 是否与"有无线程"有关。

**其他待确认**
- Delta 的 group by 默认值到底是 Project 还是 Time：文档只列选项，未说默认。`[未验证]`
- 子 thread 折叠时父行的 running indicator 具体形态（转圈？点？）`[未验证]`。

---

## 8. 参考链接

**文档（一手规格）**
- https://delta.dev/docs/delta-101.md —— 版本化产品参考（v0.24.0，2026-11-04），本文的骨架来源
- https://delta.dev/docs/agents/threads.md —— 左栏分组/排序/过滤、fork、子线程、书签、导航
- https://delta.dev/docs/agents/subagents.md —— 子 agent = 普通子 thread、profiles、follow
- https://delta.dev/docs/agents/comments.md —— 评论状态机、comment mode、发到 fork
- https://delta.dev/docs/agents/review-and-sync.md —— Changes 四基线、Seen、review verdict、Merge
- https://delta.dev/docs/agents/terminals.md —— 内联/后台终端、`@!` picker
- https://delta.dev/docs/collaboration/collaborate-thread.md —— Share 三档、邀请、草稿即聊天、跟随
- https://delta.dev/docs/collaboration/delta-on-the-web.md —— Web 能力边界、移动布局、Inbox
- https://delta.dev/docs/concepts/core-concepts.md —— thread / project / worktree 定义
- https://delta.dev/docs/configuration/keybindings.md —— 全量键位（与 delta-101 比对出矛盾）
- https://delta.dev/docs/configuration/settings.md —— Appearance / Thread 布局 / Pane / Subagents / Sidebar
- https://delta.dev/docs/privacy-and-security/agentic-safety.md —— **"没有权限系统"** 的关键证据
- https://delta.dev/docs/troubleshooting.md —— 错误态与恢复动作（diverged、worktree 失败、Xcode license…）
- https://delta.dev/docs/whats-in-the-latest.md —— 0.23/0.24 的 UI 级变更
- https://delta.dev/docs/installation.md 、 https://delta.dev/docs/getting-started/quick-start.md —— 首次运行路径
- https://delta.dev/pricing.md —— Personal / Pro，beta 状态

**博客（功能存在性佐证）**
- https://delta.dev/blog/planning-together-in-delta.md —— 多人 + 评论 + 子 thread 的真实用法
- https://delta.dev/blog/the-diff-2026-10.md —— @mentions、嵌套子 thread、多终端、移动 Web 改版
- https://delta.dev/blog/the-diff-2026-09.md —— 颜色编码、书签、Git worktree 入口
- https://delta.dev/blog/please-stop-sending-me-google-docs.md —— 评论/协作的真实截图来源

**截图（我逐张看过并据此写布局结论）**
- https://images.zed.dev/blog/the-diff/2026-09/color_code_threads.webp —— 左栏 Pinned/Projects、色条、右键菜单（真实键位）
- https://images.zed.dev/blog/the-diff/2026-09/bookmarks.webp —— 书签视图、`Bookmarks: 5`、行副标题=project
- https://images.zed.dev/blog/the-diff/2026-09/git_worktrees.webp —— **Work In 弹层**、worktree 子菜单、状态栏 `zed main`
- https://images.zed.dev/blog/the-diff/2026-10/merged_colors_and_tags.webp —— 8 色色板、`Tags…` flyout、org/Overview、标题栏四图标
- https://images.zed.dev/blog/the-diff/2026-10/nested_subthreads.webp —— 4 层嵌套树、引导线、kind 图标
- https://images.zed.dev/blog/the-diff/2026-10/multiple_terminals_2.webp —— 终端块与 `@!` Terminals picker
- https://images.zed.dev/blog/the-diff/2026-10/delta_configures_self.webp —— 完整时间线：折叠摘要行、工具行、prose 排版
- https://images.zed.dev/blog/the-diff/2026-10/web_mobile_thread_view_2.webp —— 移动端 tab bar 与 composer 控件行
- https://images.zed.dev/blog/the-diff/2026-10/web_mobile_inbox_2.webp —— 移动 Inbox 抽屉、`Unread (0) / All`、Changes 比较条
- https://images.zed.dev/blog/the-diff/2026-10/remote_mcp_servers.webp —— Settings 窗口骨架（分类列 + 表单行 + 底部按钮）
- https://images.zed.dev/blog/the-diff/2026-09/create_organization.webp —— 用户菜单：Organization 切换、计划徽标、Settings/Keymap
- https://images.zed.dev/cdn-cgi/image/width=5760,quality=75,format=auto/blog/planning-in-delta/initial-interaction-d.webp —— 左栏 Projects→thread 树、参与者头像、Thinking 折叠、行内评论锚点
- https://images.zed.dev/cdn-cgi/image/width=5760,quality=75,format=auto/blog/planning-in-delta/coles-subthread-d.webp —— **三栏 + 两个子 thread tab**、每栏独立 composer、头部 Share
- https://images.zed.dev/cdn-cgi/image/width=5760,quality=75,format=auto/blog/planning-in-delta/antonio-catch-up.webp-d.webp —— 头部动作组（Reviews/Merge/Share/右栏）、导航轨刻度、评论卡
- https://images.zed.dev/cdn-cgi/image/width=5760,quality=75,format=auto/blog/please-stop-sending-me-google-docs/img-1-d.webp —— 评论引用范围多色高亮、评论卡回复堆叠
- https://images.zed.dev/cdn-cgi/image/width=5760,quality=75,format=auto/blog/please-stop-sending-me-google-docs/img-2-d.webp —— 引用块 + 评论 composer、`Thought ›`
- https://images.zed.dev/cdn-cgi/image/width=5760,quality=75,format=auto/blog/please-stop-sending-me-google-docs/img-4-d.webp —— `No file changes yet.`、`2 comments ›`、`External changes: new commit … ›`、滚动渐隐
- https://images.zed.dev/cdn-cgi/image/width=5760,quality=75,format=auto/blog/escaping-the-tyranny-of-worktrees/img-1-l.webp —— 品牌奶油底 + mono/serif 示意图排版（非 UI）
- https://delta.dev/index.png —— OG 卡：衬线标题、暖红 accent、发丝线框（品牌排版证据）

**看过的、但与 UI 无关（避免重复劳动）**：`the-diff/2026-09/cover_photo.webp`（山湖照）、`the-diff/2026-10/ai_engineer_demoing_*.webp`、`ai_engineer_merch/team.webp`（展会照片）。
