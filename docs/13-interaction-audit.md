# 交互细节审计（全站）

基准：`docs/05-ui-spec.md`、`docs/10-ui-polish-audit.md`（视觉收口已完成，本文不重复其条目）。
方法：两个交互设计 skill——`wshobson/agents@interaction-design`（反馈/状态/动效模式）与 Vercel 的 Web Interface Guidelines。后者上游 `vercel-labs/web-interface-guidelines` 只有 `command.md`、没有 `SKILL.md`，因此安装官方封装 `vercel-labs/agent-skills@web-design-guidelines`；该 skill 每次审阅都会抓取上面那份规则清单，两者是同一套规则的两种形态。逐个路由读代码，再用 Playwright 对真实 fixture agent 会话做探针与截图。

复验入口（临时截图/探针脚本收尾已删除，行为断言固化在测试里）：

```
pnpm --dir web exec playwright test tests/e2e/aa-interactions.spec.ts
```

截图目录：`docs/screenshots/interaction/before|after/{route}-{1440|768}-{dark|light}.png`（各 24 张路由图 + 4 张审批右栏 + 4 张 New project = 32；before 另含焦点与 diff 证据）。
fixture 会话固定为：一条用户消息 + 工具卡 + agent 回复 + 一条待审批 `Write file`。截图与探针脚本是临时工具，已在收尾时删除；可复验的行为断言固化在 `web/tests/e2e/aa-interactions.spec.ts`。

严重度：**P0** 阻塞核心任务或丢数据；**P1** 常用流程的陷阱/高频摩擦；**P2** 打磨项，可排期。

---

## 1. 路由与交互面清单

| 路由 | 入口 | 快捷键 | 表单/输入 | 异步态 | 破坏性动作 | 覆盖层 |
| --- | --- | --- | --- | --- | --- | --- |
| 外壳 | TitleBar / Sidebar / StatusBar | `⌘K` `⌘⇧P` `⌘⇧J` `⌥⇧D` `⌥⇧F` `⌘⇧M` `⇧Esc` `?` | 命令面板、子线程筛选 | 侧栏/命令面板 skeleton | 无 | 覆盖式侧栏、右栏 |
| Workspace `/` | 项目列表行、Add project | — | 新建项目（目录选择） | skeleton / error / empty | 无 | New project 模态 |
| Project `/p/:id` | 线程行、New thread、Compare | — | 新建线程选项表单 | skeleton / error / empty | Archived 切换 | New thread 模态 |
| Thread `/p/:id/t/:tid` | 时间线、Composer、审批、提问、评论 | `⌘⏎` `⌘⇧E` `⌘⇧U` `⌘⌥U` `⌘⌥M` `⌘⇧S` `⌘⌥L` `⌘J` `⌃Tab` | 草稿、拒绝理由、elicitation、重命名、评论 | 草稿同步点、reconnect 条、队列 | 归档、删除、Revert、Discard、撤回排队、取消 subagent | 右栏、审批/评论面板、选区评论 |
| Inbox `/inbox` | 通知行、Unread/All | `⌘⇧J` | — | skeleton / error / empty | 无 | — |
| Settings `/settings` | 表单、审批规则 | — | 全局设置、主题 | skeleton / error | 撤销 standing rule | — |
| Compare `/p/:id/compare` | 两个 Select | — | 线程选择 | skeleton / empty | 无 | — |

不在本轮：`/login`、`/invite/:token`（无表单陷阱，仅沿用全局焦点/播报修复）；服务端 API 与信息架构。

---

## 2. 键盘可达（KB）

| ID | 位置 | 现象 | 证据 | 目标 | 级别 | 状态 |
| --- | --- | --- | --- | --- | --- | --- |
| KB-01 | `TreeRow.tsx:30` | `role=tree` 里每行 `tabIndex=0`，展开箭头也是按钮：一个项目树有 N 行就有 N+ 个 Tab 停靠点；未实现 roving tabindex | 探针 `sidebar_tree_stats`；`treeNav.ts` 只处理方向键 | 整棵树一个 Tab 入口，方向键移动，`Home/End`，`*` 展开同级 | P1 | 已修复 |
| KB-02 | `ChangesPanel.tsx:487` | diff 的每个改动行都是 `<UnstyledButton>`，逐行进入 Tab 序列；没有“聚焦行再加注释”的键盘路径 | 探针 `diff_stats`：行按钮 `tabindex=null`、`outline=none`；`docs/screenshots/interaction/before/changes-diff-1440-dark.png` | 每个 hunk 只保留一个 Tab 停靠行，方向键在行间移动；`Enter`/`Space`/`C` 在聚焦行打开评论 | P1 | 已修复（hunk 内 roving + Enter/Space/C） |
| KB-03 | `FilesPanel.tsx:104` | 文件预览的每个行号都是按钮 | 代码 | 行号改为非 Tab 停靠，容器内方向键浏览 | P2 | 待办 |
| KB-04 | `AppShell.tsx:86-121`、`WorkspaceLayout.tsx:351-445` | 全局快捷键不判断焦点：在 Composer 里按 `⌘J` 会弹出子线程选择器（草稿保留），`⌘P`/`⌘T`/`⌘W` 同理；两层 `keydown` 各自为政，没有优先级 | 探针 `ctrlJ_while_typing_dialog: 1` | 可编辑元素/已开覆盖层内只保留 `⌘⏎`、`Esc` 等局部键；`⌘P`/`⌘T`/`⌘W` 移除 | P1 | 已修复（共享 `isTypingTarget`/`isInsideOverlay` 守卫；监听器仍按各自状态归属拆分） |
| KB-05 | `SidebarHeader.tsx:46`、`WorkspaceLayout.tsx:246` | 侧栏 tooltip 宣传 `Ctrl⇧A`（New project），实际没有该 handler；`⌘W` 被绑定为“归档”，与浏览器关闭标签冲突且无法可靠拦截；`⇧Esc` 关闭抽屉不可发现 | 探针 `new_project_tooltip_live: "New project (Ctrl⇧A)"`、`ctrlShiftA_opens_dialog: 0` | 宣传的快捷键必须存在；移除 `⌘W` 归档，改为菜单/`⌘⇧A` 新项目；加 `?` 快捷键帮助面板 | P1 | 已修复 |
| KB-06 | `ProjectPage.tsx:319-337` | New thread 选项下拉 `closeOnClickOutside={false}` + `closeOnItemClick={false}`：点外部不关、`Esc` 也不关（探针实测菜单数仍为 1），只能再点一次触发器 | 探针 `new_thread_menu_open_after_outside_click: 1`、`new_thread_menu_esc: 1` | 改用 Popover 表单：外点/Esc 关闭，草稿保留，显式 Cancel | P1 | 已修复 |
| KB-07 | `NewProjectDialog.tsx:110-124` | 在路径框输入一个仓库路径后按 Enter，会进入当前高亮目录（通常是 `..`），路径从 `/tmp/threadbase-e2e/repo` 变成 `/tmp/threadbase-e2e`；不会提交 | 探针 `new_project_enter` | 输入完整可识别路径时 Enter = 添加；列表导航改为 `↑↓` 聚焦、`→` 进入 | P1 | 已修复 |
| KB-08 | `Composer.tsx:319-338` | 发送键只在 `sendWithModifier=false` 时是 Enter；界面没有说明当前模式 | 代码；`docs/10` 未覆盖 | 在 Composer 底部显示 `⏎ 发送 / ⇧⏎ 换行` 或 `⌘⏎`，随设置变化 | P2 | 待办 |
| KB-09 | `ThreadPage.tsx:840`、`ShareDialog.tsx:335` | 重命名、邀请成员等单行表单不响应 Enter | 代码 | 单行表单 Enter 提交，`Esc` 取消 | P2 | 部分修复（重命名 Enter 已加；邀请表单仍待） |
| KB-10 | `treeNav.ts` | 无 type-ahead（输入字母跳转） | 代码；`←/→`、`Home/End`、`*` 已实现 | 输入字母跳转到匹配行 | P2 | 部分修复（方向键/`Home/End`/`*` 已有；type-ahead 待） |

---

## 3. 焦点可见与焦点管理（FC）

| ID | 位置 | 现象 | 证据 | 目标 | 级别 | 状态 |
| --- | --- | --- | --- | --- | --- | --- |
| FC-01 | `TreeRow.tsx:42` | 树行 `outline: none` 且无替代焦点环：键盘聚焦后唯一变化是选中底色，当前行与选中行无法区分 | 探针 `tree_row_focus`（`outlineStyle: none`、`boxShadow: none`）；`before/focus-tree-row-1440-dark.png` | 全局 `:focus-visible` 令牌（2px + offset），树行/自定义按钮统一使用 | P0 | 已修复 |
| FC-02 | `index.css:170` | `.tb-focus-ring` 定义了却无人使用；Mantine 组件有焦点环，自定义元素没有 | `grep tb-focus-ring` = 0 引用 | 删除死类，改为全局 `:focus-visible` 规则 | P1 | 已修复 |
| FC-03 | `index.css:160` | 无 `prefers-reduced-motion` 的焦点/动效统一策略 | 见 MO-01 | 与 MO 一起处理 | P2 | 已修复 |
| FC-04 | `AppShell.tsx:169-235` | 覆盖式侧栏打开时焦点留在主内容；关闭后不还给触发按钮。桌面右栏关闭同样不还焦点 | 代码；`aria-hidden/inert` 已正确但无 focus 管理 | 打开时聚焦第一个可交互项，关闭时还焦；桌面右栏打开聚焦当前 tab | P1 | 已修复（侧栏/右栏打开均移焦，关闭均还焦） |
| FC-05 | `SelectionComment.tsx:40` | 浮动评论框无焦点陷阱、无外点关闭；关闭后焦点落到 body | 代码 | 外点关闭、Esc 关闭并把焦点还给选区所在消息 | P2 | 部分修复（外点关闭已加；关闭后焦点归还待） |
| FC-06 | `CommandPalette.tsx` | Spotlight 自带焦点管理，但结果区 loading/empty 变化无播报 | 见 FB-09 | 与 FB 一起处理 | P2 | 已修复 |
| FC-07 | `AppShell.tsx:143-166` | 右栏/侧栏按钮有 `aria-expanded`，但侧栏覆盖层没有 `aria-modal` 语义 | 代码 | 覆盖层加 `role=dialog aria-modal`，或保持非模态但加 `aria-controls` | P2 | 待办 |
| FC-08 | `NewProjectDialog.tsx:150-190` | 目录高亮只跟随鼠标；键盘上下移动时列表不滚动到可见区 | 代码 | `aria-activedescendant` + `scrollIntoView({block:"nearest"})` | P2 | 待办 |

---

## 4. 操作反馈与异步状态（FB）

| ID | 位置 | 现象 | 证据 | 目标 | 级别 | 状态 |
| --- | --- | --- | --- | --- | --- | --- |
| FB-01 | `ThreadPage.tsx` 多处 mutation | archive / fork / restart / configure / cancelSubagent / elicitation 无 `onError`：失败时界面无任何反馈 | 代码 | 统一 `notifyError()`；所有写操作失败都有 toast/内联错误 | P1 | 已修复 |
| FB-02 | `ApprovalRow.tsx:190-230`、`RightDrawer.tsx:500-535` | 审批按钮不进入 pending/disabled：连点会重复提交；拒绝意见先清空再请求 | 代码；`decision` 只传 `onDecide`，无 `isPending` | 每张卡按 approval id 禁用并 loading；成功后清空、失败保留意见 | P1 | 已修复 |
| FB-03 | `RightDrawer.tsx:470-495` | 拒绝理由在点击“Deny with reason”后立即清空，请求失败无法恢复 | 代码 | 失败保留文本并显示错误 | P1 | 已修复 |
| FB-04 | `ElicitationCard.tsx:225-250` | Submit/Decline/Cancel 无 pending；必填字段不校验；数字输入清空会变成 `0` | 代码 | pending 禁用、必填校验、空数字保持 `undefined` | P1 | 已修复 |
| FB-05 | `ThreadTimeline.tsx` | 时间线没有贴底/新消息提示：新事件到达时视口不滚动，也没有“N 条新消息”入口 | `grep scrollTo` 无命中；聊天核心交互 | 贴底时自动滚；离开底部时显示“跳到底部 / N 条新消息” | P0 | 已修复 |
| FB-06 | `ThreadPage.tsx:520-545` | 离线/reconnecting 与“历史被压缩”横幅不是 `aria-live`，屏幕阅读器不播报 | `grep aria-live` = 0 | 加 `role=status aria-live=polite` | P1 | 已修复 |
| FB-07 | `StatusBar.tsx:203-240` | `+0 −0` 空计数常驻；`0 running / 0 waiting` 靠 `title` 才可读；连接状态无播报 | 截图 `thread-1440-dark.png` 底部 | 零计数隐藏或合并为“空闲”；状态文本可读 | P2 | 待办 |
| FB-08 | `Composer.tsx:436-455` | 草稿同步圆点 `aria-hidden` + `title`：没有可读状态；未同步时 Send 禁用但不说原因 | 截图 Composer 左下 | 文本状态（Synced/Syncing/Offline），禁用时 `title`/`aria-describedby` | P2 | 待办 |
| FB-09 | `CommandPalette.tsx:100-175` | 搜索 loading/无结果/失败区没有播报；结果数量不告知 | 代码 | `role=status` 播报“3 个结果 / 无结果” | P2 | 已修复 |
| FB-10 | `ProjectPage.tsx:420-440` | “New thread” 点击后按钮无 loading，创建中可再次点击（仅靠 mutation 静默去重） | 代码 | 按钮 loading + 文本“Creating…”，成功后跳转 | P2 | 待办 |
| FB-11 | `SettingsPage.tsx:173-320` | 设置表单无“已修改”状态，Save 常亮；离开页面丢改动 | 代码 | 脏值追踪、Save 仅在有改动时启用、离开前提示 | P1 | 已修复 |
| FB-12 | `CommentsPanel.tsx:130-200` | “Save pending / Send now” 共用 loading；无 `⌘⏎`；删除无反馈 | 代码 | 明确两种提交结果，快捷键与错误反馈 | P2 | 部分修复（⌘⏎ 与删除确认已加；双 loading 文案待） |

---

## 5. 破坏性操作与撤销（DS）

| ID | 位置 | 现象 | 证据 | 目标 | 级别 | 状态 |
| --- | --- | --- | --- | --- | --- | --- |
| DS-01 | `ShareDialog.tsx:236-250`、`:288-300` | 移除成员、吊销邀请即时生效，无确认、无撤销 | 代码 | 确认对话框（说明影响）+ 成功 toast/撤销入口 | P1 | 已修复 |
| DS-02 | `CommentsPanel.tsx:63-73` | 删除评论即时生效，无确认/撤销 | 代码 | 确认或 5s 撤销 toast | P1 | 已修复 |
| DS-03 | `ChangesPanel.tsx:317-352` | Revert/Apply 使用原生 `window.confirm`：阻塞、不可主题化、按钮文案不可定制 | 代码 | 统一 ConfirmDialog，破坏性按钮红色、说明后果 | P1 | 已修复 |
| DS-04 | `ThreadPage.tsx:915-925` | diverged 横幅的“Discard”直接回滚，无确认 | 代码 | 确认后执行，失败保留横幅 | P1 | 已修复 |
| DS-05 | `Composer.tsx:445-470` | 队列“withdraw”即时移除，无确认/撤销 | 代码 | 撤销 toast 或确认 | P2 | 待办 |
| DS-06 | `SubagentView.tsx:150-158` | “Cancel subagent” 即时取消，无确认、无 pending | 代码 | 确认 + pending | P2 | 待办 |
| DS-07 | `ApprovalRow.tsx:205` | “Always allow / Allow for thread” 会创建持久规则，卡片上没有说明范围与撤销路径 | 截图 `thread-approvals-1440-dark.png` | 文案说明“对本线程后续同类调用生效，可在 Settings 撤销” | P2 | 待办 |
| DS-08 | `ThreadPage.tsx:700-720` | 删除线程有确认（好），但菜单项对只读成员仍可点，失败才报错 | 代码 | 按 `canWrite` 禁用并说明原因 | P2 | 待办 |
| DS-09 | `ThreadPage.tsx:780-800` | “Stop”即时取消；同页“Stop & send”却要二次确认，语义不一致 | 代码 | 统一：Stop 也确认，或 Stop & send 直接执行 | P2 | 待办 |
| DS-10 | `ApprovalRulesSection.tsx:103` | 撤销 standing rule 无确认 | 代码 | 与 DS-01 同一确认组件 | P2 | 待办 |

---

## 6. 动效与 reduced-motion（MO）

| ID | 位置 | 现象 | 证据 | 目标 | 级别 | 状态 |
| --- | --- | --- | --- | --- | --- | --- |
| MO-01 | `index.css` | 没有 `@media (prefers-reduced-motion: reduce)`；自定义过渡/关键帧在 reduce 下仍执行。探针：`rowTransition: background-color .12s`、`chevronTransition: transform .12s` | 探针 `reduced_motion`（`reduced: true`） | 全局 reduce 块：`animation/transition-duration: 0.01ms`，保留焦点环 | P1 | 已修复 |
| MO-02 | `rowStyles.ts:2`、`TreeRow.tsx:120` | 动效数值硬编码 120ms/150ms，未用 `lib/design/tokens.ts` 的 `transitions` | 代码 | 统一 token；时长/缓动可审计 | P2 | 部分修复（树行/侧栏已走变量；其余硬编码待） |
| MO-03 | `index.css:122-200` | 7 个 keyframes + 4 个 `.tb-animate-*` 工具类无引用（死代码） | `grep tb-animate` = 0 | 删除或接入（如骨架淡入） | P2 | 待办 |
| MO-04 | `ThreadPage.tsx:760` | `animation: "pulse 2s infinite"` 引用的 `pulse` keyframes 不存在，live turn 圆点不动 | 探针 `pulseAnimationName: null`；`index.css` 只有 `sidebar-pulse` | 用 `sidebar-pulse` 或定义 `pulse`；reduce 下关闭 | P2 | 已修复 |
| MO-05 | `AppShell.tsx:218-232` | 侧栏宽度/位移过渡 150ms 硬编码；覆盖层背景直接出现，无淡入 | 代码 | 统一 200ms `transitions.fast`，reduce 下 0 | P2 | 部分修复（侧栏时长走变量；遮罩淡入待） |

---

## 7. 外壳（SH）

| ID | 位置 | 现象 | 证据 | 目标 | 级别 | 状态 |
| --- | --- | --- | --- | --- | --- | --- |
| SH-01 | `TitleBar.tsx:66-100` | 计算了 `subtitle`（project / branch）但标题栏只渲染 `title`，thread 上下文丢失；`docs/10` SH1 的回归 | 截图 `thread-1440-dark.png` 顶栏只有标题；`WorkspaceLayout.tsx:477` 传入 subtitle | 渲染副标题，窄屏截断 | P1 | 已修复 |
| SH-02 | `TitleBar.tsx:106` | `modKeyCombo("K")` 渲染为 `CtrlK`，缺 `+` | 截图左上角 | `Ctrl+K` | P2 | 已修复 |
| SH-03 | `TitleBar.tsx:66-80` | 搜索按钮视觉上只有一个 `Kbd`，没有 “Search” 文案/图标语义（图标在沙箱里被 Kbd 挤掉） | 截图 | 图标 + “Search” + Kbd 三段式 | P2 | 已修复 |
| SH-04 | `TitleBar.tsx:186` | Share 的 tooltip 写 “Copy link to this thread”，实际打开分享面板 | 代码 | 文案改为 “Share thread”，与面板一致 | P2 | 已修复 |
| SH-05 | `TitleBar.tsx:210-230` | 审批图标打开右栏但没有 `aria-expanded/aria-controls`；角标仅颜色区分 | 代码 | 补状态属性与可读计数 | P2 | 部分修复（已加 aria-haspopup；aria-expanded/controls 待） |
| SH-06 | `SidebarTree.tsx:255` | “Show N more” 单向展开，无法收回到 10 条；列表长时没有虚拟化 | 代码 | 切换按钮（Show more/Less），长列表虚拟化 | P2 | 待办 |
| SH-07 | `WorkspacePage.tsx:125` | “No projects” 空态没有 Add project 主行动 | 代码 | 空态内嵌 Add project | P2 | 待办 |

---

## 8. 页面与 Thread 细节（PG / TH）

| ID | 位置 | 现象 | 证据 | 目标 | 级别 | 状态 |
| --- | --- | --- | --- | --- | --- | --- |
| PG-01 | `ProjectPage.tsx:420` | 线程空态无 “New thread” CTA；Archived 空态文案仍是 “No threads” | 代码 | 空态按筛选给正确文案 + CTA | P2 | 已修复 |
| PG-02 | `InboxPage.tsx:120` | 切换 Unread/All 无过渡说明；Unread 有计数，All 没有 | 代码 | 两个 tab 都显示计数，切换保留滚动位置 | P2 | 待办 |
| PG-03 | `ComparePage.tsx:150-200` | 左右可选同一线程；选择不写入 URL，刷新丢失；少于 2 条线程时没有 CTA | 代码 | 去重选择、`?left=&right=` 持久化、空态 CTA | P2 | 待办 |
| PG-04 | `SettingsPage.tsx:220` | `subthreads-per-thread` 可输入任意值（1–100 之外只靠服务端报错）；webhook 无格式校验 | 代码 | 客户端范围/URL 校验与内联错误 | P2 | 待办 |
| TH-01 | `ApprovalRow.tsx:205-230`、`RightDrawer.tsx:500-535` | 审批三按钮里只有 “Reject” 是 filled 红色，视觉上成了默认主操作；Allow 选项是 outline | 截图 `thread-approvals-1440-dark.png`（`Reject` 背景 `rgb(240,62,62)`，其余透明） | Reject 降为 `light`/outline；Allow once 用 primary；危险色只留给确认后的动作 | P1 | 已修复 |
| TH-02 | `ThreadPage.tsx:830-850` | 重命名表单 Enter 不提交 | 代码 | Enter 保存 | P2 | 已修复 |
| TH-03 | `MessageRow.tsx:70-105` | 每行两个语义相近的 24px 图标（View comments / Comment），无 hover 收敛；评论数不可见 | 截图；`docs/10` TH6 视觉项 | 合并为一个“评论”入口（带计数），hover/focus 才显示 | P2 | 待办 |
| TH-04 | `ThreadPage.tsx:120-175` | 选区评论只监听 `mouseup`（触屏选词不触发）；无外点关闭；创建失败无反馈；提示写死 `⌘` | 代码 | `selectionchange`/触屏支持、外点关闭、错误提示、平台键位 | P1 | 已修复 |
| TH-05 | `ThreadTimeline.tsx` | 工具组 `<details>` 的箭头不随展开旋转，没有 `aria-expanded` 语义 | 截图 | 旋转 + 原生 details 已可键盘 | P2 | 待办 |
| TH-06 | `Composer.tsx:640-680` | “Stop & send” 需要二次点击确认，但确认态除文案外无样式差异，且不会超时复位 | 代码 | 确认态用 warning 色 + 3s 超时复位 | P2 | 待办 |
| TH-07 | `ThreadPage.tsx:880-900` | 只读用户 “Request access” 可重复点击，无 pending/已发送状态 | 代码 | 发送后禁用并显示 “Requested” | P2 | 待办 |
| TH-08 | `Composer.tsx:585-600` | 队列 “continue” / “withdraw” 两个按钮紧邻，无危险区分 | 代码 | withdraw 用危险样式并确认 | P2 | 待办 |
| TH-09 | `ThreadPage.tsx:600-625` | subagent 选择器过滤框只有 placeholder，无 label | 代码 | `aria-label="Filter subthreads"` | P2 | 待办 |
| TH-10 | `ThreadPage.tsx:260-300` | 挂载即标记已读；`⌘⇧U` 切换已读无任何反馈 | 代码 | toast/状态徽标变化可见 | P2 | 待办 |

---

## 9. 优先级与归口

| 任务 | 覆盖条目 |
| --- | --- |
| `global-interaction` | FC-01、FC-02、FB-06、MO-01、MO-02、MO-04、MO-05 |
| `shell-interactions` | KB-01、KB-04、KB-05、KB-10、FC-04、FC-07、SH-01…SH-05 |
| `page-interactions` | KB-06、KB-07、KB-08、KB-09、FB-10、FB-11、DS-01、DS-03、PG-01…PG-04 |
| `thread-interactions` | KB-02、FB-02…FB-05、FB-08、FB-12、DS-02、DS-04…DS-09、TH-01…TH-10 |
| `interaction-e2e` | KB-01、KB-04、KB-06、KB-07、FB-02、FB-05、MO-01、TH-01 |

P0（必须先修）：FC-01、FB-05。
P1（本轮修）：KB-01、KB-02、KB-04、KB-05、KB-06、KB-07、FC-02、FC-04、FB-01、FB-02、FB-03、FB-04、FB-06、FB-11、DS-01、DS-02、DS-03、DS-04、MO-01、SH-01、TH-01、TH-04。
P2 进入 backlog，逐条保留 ID 与证据，不阻塞本轮收口。

## 10. 复验计划

1. 上述 P0/P1 在 §11 有状态与实现说明；部分修复条目注明剩余点。
2. before/after 截图已生成在工作区 `docs/screenshots/interaction/`（本轮交付包含该目录），按路由 × 1440/768 × dark/light 成对。
3. 行为复验以 e2e 为准：`pnpm --dir web exec playwright test`（6 passed），覆盖树 roving/焦点环、快捷键焦点保护（含模态内）、New project Enter、评论确认、Settings 离页、时间线贴底/Jump to latest、审批 pending/失败保留、右栏开关焦点。截图与探针脚本是临时工具，收尾已删除。
4. `scripts/verify.sh --with-e2e` 全绿；`git status` 无临时探针/截图脚本。

---

## 11. 收口记录（2026-11-11）

### 11.1 已修复（P0/P1 全部）

| 级别 | 条目 |
| --- | --- |
| P0 | FC-01 焦点环、FB-05 时间线贴底/跳到底部 |
| P1 | KB-01/02/04/05/06/07、FC-02/04、FB-01/02/03/04/06/11、DS-01/02/03/04、MO-01、SH-01、TH-01/04 |

关键实现：

- `index.css`：全局 `:focus-visible` 环（覆盖 Mantine `mantine-focus-auto`）、`--tb-duration-*` 动效变量、`prefers-reduced-motion` 归零。
- `lib/shortcuts.ts`：键盘目标判定 + 快捷键清单；`?` 打开帮助面板。`⌘W` 归档移除，`⌘⇧A` 与 tooltip 一致，`⌘⇧E` 归档。
- `SidebarTree` roving tabindex + `inert` 覆盖层 + 焦点归还；`ArrowRight` 进入子行。
- `NewProjectDialog` Enter 优先提交；`ProjectPage` 选项表单改为 Modal（外点/Esc 可关）。
- `ThreadPage`：贴底自动滚动、`Jump to latest`、审批 pending、Discard 确认、失败统一 `notifyError`；Files 面板评论 mutation 同样补了失败 toast。
- `ApprovalRow`/`RightDrawer`：`allow_once` 为主按钮，拒绝降为 outline/subtle；拒绝理由失败保留。
- `ConfirmDialog`：替换 `window.confirm`，成员移除/邀请吊销/评论删除/Revert/Apply/Discard 统一确认。
- `SettingsForm`：脏值状态、Save 仅在有改动时可用、`beforeunload` 保护。

### 11.2 复验证据

```
pnpm --dir web typecheck   # 0 error
pnpm --dir web lint        # 0 error 0 warning
pnpm --dir web build       # ok
pnpm --dir web exec playwright test   # 6 passed
scripts/verify.sh --with-e2e          # verify: PASS
```

e2e 固化（`web/tests/e2e/aa-interactions.spec.ts` + core 既有流程）：

- 树只有一个 Tab 停靠点、焦点环存在、reduce 下 `transitionDuration` 全为 `0s`。
- Composer 内 `⌘J` 不弹窗；`⌘K` 可打开并输入/关闭命令面板。
- New project 输入仓库路径后 Enter 提交（不再走到父目录）。
- 审批：`Allow once` 为 `filled`，拒绝不是；延迟 decision 响应时按钮 `disabled`（真实 pending），完成后消失。
- 审批失败路径：故意让 decision 返回 500，拒绝理由仍保留在输入框，并有错误 toast；重试成功。
- 评论删除弹出确认，取消后评论保留；再次确认后删除。
- Settings 未保存改动：站内导航弹出 “Discard unsaved settings?”，取消留在原页，确认才跳转。
- 时间线：连续事件后仍贴底；滚到顶部出现 “Jump to latest”，点击回到底部。
- Composer 内 `⌘J` / `⌘⌥M` 不再触发全局动作；New project 模态打开时（焦点在对话框按钮上）`⌘J` 也不穿透。
- 打开右栏后焦点落在当前 tab；关闭后回到触发按钮（两者均有 `toBeFocused` 断言）。
- diff 行键盘路径：e2e 断言每个 hunk 仅 1 个 Tab 停靠行，并逐一验证 `C`/`Enter`/`Space` 都能打开该行评论。
- 截图 after 生成于最终代码之后（18:10，晚于最后一次源码修改 18:08）。

### 11.3 未修条目（P2 backlog）

KB-03/08、FC-07/08、FB-07/08/10、DS-05/06/07/08/09/10、MO-03、SH-06/07、PG-02/03/04、TH-03/05/06/07/08/09/10。均已带 ID、证据与目标留在上表，不阻塞本轮收口；`部分修复` 条目（KB-09、KB-10、FC-05、FB-12、MO-02、MO-05、SH-05）在 11.1 的范围内已覆盖主路径。
