# UI 细节审计（v1 视觉收口）

基准：`docs/05-ui-spec.md` §1 / §10（Linear / Apple Notes 方向）、§3 行高与缩进、§4.2 时间线元素。
方法：vite dev server（`127.0.0.1:5173`）+ 本地服务（`127.0.0.1:7777`，Tailscale 身份头 `tailscale-user-login`），Playwright 逐路由截图，再用 `getComputedStyle` / `getBoundingClientRect` 探针取精确数值。

截图：`docs/screenshots/ui-polish/before/{route}-{1440|768}-{dark|light}.png`（36 张）。
生成脚本：`web/.ui-shots.mjs`（临时工具，收尾删除）；探针：`web/.ui-probe*.mjs`。

---

## 1. 全局问题

| ID | 现象 | 证据 | 代码位置 | 类型 |
| --- | --- | --- | --- | --- |
| G1 | 主色是 Mantine 默认蓝（`#228be6` / dark `#1971c2`），出现在主按钮、Tab 下划线、焦点环、链接、审批角标；与 §10「单色 primary」不符 | 全部截图；`primaryVar` 探针 | `main.tsx:25`（`createTheme` 只设了 `fontFamily`） | 色板/层级 |
| G2 | 列表行用 `<Link>` 包 `<Text>`，没有重置颜色 → 行标题吃浏览器默认链接色：dark `rgb(158,158,255)`、light `rgb(0,0,238)`。浅色下项目/线程/Inbox 行标题全是蓝紫色 | 探针 `rowTitleColor`；`project-1440-light.png` | `WorkspacePage.tsx:26`、`ProjectPage.tsx:61`、`InboxPage.tsx:61` | 颜色/可读性 |
| G3 | 页面内容左缘五套：Inbox 头 338、Workspace/Project 444、Thread 492、Settings 508、Compare 316 | 探针 rect | 见各页条目 | 对齐 |
| G4 | 页面纵向 padding 五套：`xl`(32) / `lg`(24) / `md`(16) 混用 | `WorkspacePage.tsx:76`、`SubagentComparePage.tsx:42`、`InboxPage.tsx:132` | 同左 | 间距 |
| G5 | 「列表行」三种密度：Workspace 项目行 `py=md`（72px）、Project/Inbox 行 `py=sm`（64px）、侧栏行 32px | 探针 `rowPadding: 16px` / `rowBox.h:64` | `WorkspacePage.tsx:31`、`ProjectPage.tsx:70`、`InboxPage.tsx:66`、`SidebarTree.tsx:270` | 行密度 |
| G6 | 圆角混用：主体 8px（`md`），但 Composer 32px（`xl`）、队列面板 16px（`lg`）、对比摘要 4px、空态 `ThemeIcon radius=md` | 截图；`Composer.tsx:413`、`ComparePage.tsx:143` | 同左 | 圆角 |
| G7 | 字号出现 11.5 / 12 / 13 / 14 混排，作者行 11.5px 是唯一非整数；正文 13px 而工具行 14px | `ThreadTimeline.tsx:840`、`ThreadTimeline.tsx:1290` | 同左 | 字号层级 |
| G8 | 图标尺寸 10 / 12 / 14 / 16 无规则，同一动作跨页不同尺寸（New thread 的 `+` 在侧栏 12、项目页按钮内 18） | 截图 | 各页 | 图标 |

## 2. 外壳（TitleBar / Sidebar / StatusBar / RightDrawer）

| ID | 现象 | 证据 | 代码位置 | 类型 |
| --- | --- | --- | --- | --- |
| SH1 | `subtitle` 声明但从未渲染 → Thread 页标题栏只剩标题，丢了 `project / branch` | `titlebar` 探针 `headerTitle.w=28` | `TitleBar.tsx:22`（声明），`:66-120`（渲染） | 信息缺失 |
| SH2 | `{modKeyLabel()}K` 拼成 `CtrlK`，缺分隔符 | `workspace-1440-dark.png` 左上角 | `TitleBar.tsx:106` | 文案/排版 |
| SH3 | 无 thread 的页面（Workspace/Inbox/Settings）仍显示禁用态 Share，多一个灰按钮；审批入口用 `CheckCheck`（双勾=已完成）表达"待审批" | 截图三页 | `TitleBar.tsx:170-205` | 一页一主动作 / 图标语义 |
| SH4 | 右上角三个控件高矮不齐：Share 36px，两个 `ActionIcon size="sm"` 28px；审批角标白字压黄底，且 `-2/-2` 溢出图标 | 截图右上角 | `TitleBar.tsx:168-230` | 对齐 |
| SH5 | 侧栏选中行没有高亮：`var(--mantine-color-primary-light)` 未定义，计算值 `rgba(0,0,0,0)` | 探针 `selectedRow.bg` | `SidebarTree.tsx:281` | 状态不可见 |
| SH6 | 侧栏行高 32、每层缩进 14px，与 §3 的 34–36 / 16 不符；`paddingRight:6` / `paddingLeft:4+14d` 不对称 | 截图树行 | `SidebarTree.tsx:264-282` | 行高/缩进 |
| SH7 | 侧栏自带 48px「Threadbase」品牌行，与标题栏重复且高度 48 ≠ 44，左右内边距 16 ≠ 标题栏 12 | `thread-1440-dark.png` 左上 | `SidebarTree.tsx:1105-1120`、`TitleBar.tsx:66-72` | 重复/对齐 |
| SH8 | 状态栏三份截断路径（project / branch / worktree），1440 也截；worktree 段是 ULID 片段，无语义 | `thread-1440-dark.png` 底部 | `StatusBar.tsx:57-160` | 信息密度 |
| SH9 | 状态栏 `+0` `−0` 之间无间距，且是 `Button` 里套两个 `Text`；图标 `FileDiff` 在 12px 下像书本 | 截图底部 | `StatusBar.tsx:203-230` | 对齐/图标 |
| SH10 | 两个计数器视觉语言不同：running 是 Loader/CircleDot，waiting 是 6px 圆点 | 截图底部 | `StatusBar.tsx:231-262` | 一致性 |
| SH11 | 右栏抽屉里头部 `paddingInline:16`、Tab 区 `paddingInline:12`、未选中态图标按钮无 label 底色，三段内边距不一致 | `RightDrawer.tsx:129-205` | 同左 | 间距 |

## 3. Thread

| ID | 现象 | 证据 | 代码位置 | 类型 |
| --- | --- | --- | --- | --- |
| TH1 | Composer 卡 x=476 w=768，时间线正文列 x=492 w=736 → 左右各多 16px；与消息气泡、工具卡都不共边 | 探针 `composerChildWidths` / `firstBubble` / `noticeRow` | `Composer.tsx:316-345` 与 `ThreadTimeline.tsx:791-795` | 对齐 |
| TH2 | Composer `radius="xl"`(32) + `minHeight:148`，1440 下是个 201px 高的空盒子；空编辑器没有 placeholder，看起来像坏掉 | `thread-1440-dark.png` | `Composer.tsx:412-420` | 密度/空态 |
| TH3 | `Usage` / `Turn ended` 这类 meta 行渲染成 42px 高、带底色的卡片行，每行右侧固定一个 `CheckCircle2`（error 语气也打勾） | `thread-1440-dark.png` 时间线 | `ThreadTimeline.tsx:1544-1595`（`CheckCircle2` 在 1587） | 图标语义/密度 |
| TH4 | 时间线 `Stack gap="md"`(16) 对 meta 行过大，导致连续 4 条 Usage 像卡片列表 | 截图 | `ThreadTimeline.tsx:795` | 间距 |
| TH5 | 消息气泡 `px=sm py=xs`、notice 行同、工具卡 summary `8/12` —— 三种内边距；作者行 11.5px 与正文 13px 差 1.5px | `ThreadTimeline.tsx:836-880`、`:1290` | 同左 | 字号/间距 |
| TH6 | 每条消息右上两个 18px 点击区的图标按钮（`MessageSquareText` / `MessageSquare`），语义相近且低于 24px 最小点击区 | `thread-1440-dark.png` | `ThreadTimeline.tsx:855-880` | 可点击性 |
| TH7 | Thread 内页头皮 `height` 与标题栏同为 44，左内边距 12 与时间线 16 不一致，两道分隔线相距 44px | `thread-1440-dark.png` y=44/88 | `ThreadPage.tsx` 头部分组 | 对齐 |
| TH8 | SubagentView 里 Compare 与 Back to thread 都用 `Layers` | `thread-*`（进入子会话时） | `ThreadPage.tsx:997`、`ThreadPage.tsx:1018` | 图标语义 |
| TH9 | Thread 错误态用 `Paper` 包 `ErrorState` + 自绘分隔线，与 NotFound/Inbox 的空错状态结构不同 | `ThreadPage.tsx:493-520` | 同左 | 结构一致性 |

## 4. Project

| ID | 现象 | 证据 | 代码位置 | 类型 |
| --- | --- | --- | --- | --- |
| PR1 | Compare 按钮用 `ArrowLeftRight`（互换），在 Project 与 Compare 页两处都读作"切换/交换" | `project-1440-dark.png` | `ProjectPage.tsx:310`、`ComparePage.tsx:23` | 图标语义 |
| PR2 | 行右侧两行对齐：状态徽章在标题行，diff/头像在副标题行 → 右缘锯齿，diff 与徽章只隔 8px | `project-1440-dark.png` 行尾 | `ProjectPage.tsx:75-140` | 对齐 |
| PR3 | 拆分按钮两半 `gap={0}` 且各自截断圆角，hover 时出现接缝；`paddingLeft/Right:8` 的 chevron 段比主按钮窄 10px | 截图 | `ProjectPage.tsx:324-352` | 组件细节 |
| PR4 | 页头标题 + 3 个按钮同排，768 下标题被压到 38px 起；按钮簇无 `wrap` 降级 | `project-768-*.png` | `ProjectPage.tsx:263-305` | 响应式 |
| PR5 | diff 数字两处写法不同：行内 `<Text>` 嵌套 + `{" "}`，汇总行用 `mx=6` 的 `·` | `ProjectPage.tsx:120-137` vs `:380-395` | 同左 | 一致性 |
| PR6 | 空态里再给一个 `New thread` 按钮，与页头动作重复 | `ProjectPage.tsx:429-440` | 同左 | 一页一主动作 |

## 5. Workspace

| ID | 现象 | 证据 | 代码位置 | 类型 |
| --- | --- | --- | --- | --- |
| WS1 | 时间列固定 `w={64}`，768 下 "11 minutes ago" 断成两行 | `workspace-768-*.png`（"11 minutes / ago"） | `WorkspacePage.tsx:84` | 破版 |
| WS2 | 项目行 `py="md"` → 72px；Project 页线程行 `py="sm"` → 64px | 探针 | `WorkspacePage.tsx:31`、`ProjectPage.tsx:70` | 行密度 |
| WS3 | 空态再放一个 `Add project`，与页头重复 | `WorkspacePage.tsx:100-120` | 同左 | 一页一主动作 |
| WS4 | 右侧 meta 在 `<sm` 全部隐藏（`visibleFrom="sm"`），无替代信息 | `WorkspacePage.tsx:66` | 同左 | 响应式 |

## 6. Inbox

| ID | 现象 | 证据 | 代码位置 | 类型 |
| --- | --- | --- | --- | --- |
| IB1 | 头部标题左缘 338，列表容器左缘 476 → 138px 错位；头部没有自己的最大宽度 | 探针 `headTitle` / 列表 maw | `InboxPage.tsx:129-166` vs `:189` | 对齐 |
| IB2 | 头部 `borderBottom` 与 `Tabs.List` 下边框叠成双线 | `inbox-1440-dark.png` y≈139 | `InboxPage.tsx:135` + `:146` | 分隔线 |
| IB3 | Tab 内再嵌 `Group` + 计数 `Text`，渲染成 "Unread␠␠(0)"，字号/颜色跳变 | 截图 | `InboxPage.tsx:148-160` | 排版 |
| IB4 | 同一条目用图标 + 6 种颜色 badge 双重编码 kind，颜色系统与主色冲突 | 截图 | `InboxPage.tsx:25-42` | 色板 |
| IB5 | 未读点 `marginTop:6` 与 24px 头像不居中对齐 | `InboxPage.tsx:104-112` | 同左 | 对齐 |

## 7. Settings

| ID | 现象 | 证据 | 代码位置 | 类型 |
| --- | --- | --- | --- | --- |
| ST1 | 容器 `maw=768`，Workspace/Project 是 896 → 左缘差 64px | 探针 `cards[0].x=508` | `SettingsPage.tsx:279` | 对齐 |
| ST2 | 卡片头 32×32 灰底图标块看起来像按钮，实际不可点 | `settings-1440-dark.png` | `SettingsPage.tsx:59-62` | 图标/层级 |
| ST3 | 两列网格把「Subthreads per thread」（数字）和「Send with modifier key」（勾选）凑成一行，语义不成对 | 截图 | `SettingsPage.tsx:330-360` | 布局 |
| ST4 | 数字输入框占满 253px，`maw={80}` 只用在别处 | 截图 | `SettingsPage.tsx:105` vs `:340` | 尺寸 |
| ST5 | 页面滚动到底，最后一张卡被状态栏压住（无底部留白） | `settings-1440-dark.png` 底部 | `SettingsPage.tsx:279`（`py="xl"` 不足） | 间距 |

## 8. Compare

| ID | 现象 | 证据 | 代码位置 | 类型 |
| --- | --- | --- | --- | --- |
| CP1 | 容器 `maw=1152`，与其它页五套宽度又不同；SubagentCompare 是 1152 + `px="md"` | `ComparePage.tsx:176`、`SubagentComparePage.tsx:42` | 同左 | 对齐 |
| CP2 | 两个 `Select` 无可见标签，宽 224，右对齐；与下方两列卡片不共边 | `compare-1440-dark.png` | `ComparePage.tsx:197-220` | 对齐/表单 |
| CP3 | 「Open thread」是默认蓝链接，与行标题的浏览器蓝撞色 | 截图 | `ComparePage.tsx:160-172` | 颜色 |
| CP4 | 摘要块 `borderRadius: 4`，与全局 8px 不一致 | `ComparePage.tsx:143` | 同左 | 圆角 |

## 9. Login / NotFound / Invite

| ID | 现象 | 证据 | 代码位置 | 类型 |
| --- | --- | --- | --- | --- |
| LG1 | 主题切换按钮浮在视口右上 12px，30px 大小的孤立圆按钮；其它页在侧栏底部 | `login-1440-dark.png` | `LoginPage.tsx:160-162` | 位置一致性 |
| LG2 | 用户名占位符 `ada` 看起来像已填内容 | 截图 | `LoginPage.tsx` 表单 placeholder | 文案 |
| LG3 | 品牌行居中、标题左对齐、表单 420 宽，三条对齐轴 | 截图 | `LoginPage.tsx:163-180`、`:43` | 对齐 |
| LG4 | 满宽蓝色主按钮 + 蓝色焦点环 | 截图 | G1 派生 | 颜色 |
| NF1 | NotFound 的标题栏仍显示 "Projects"（`pageTitle` 兜底），无面包屑 | `notfound-1440-dark.png` | `WorkspaceLayout.tsx:57-76` | 标题 |
| NF2 | Invite 直接渲染 Rust 错误串 `ApiError: origin is not allowed` | `invite-1440-dark.png` | `InvitePage.tsx` 错误渲染 | 文案 |
| NF3 | 空/错状态三种结构：`EmptyState` / `Paper + ErrorState + 分隔线` / 裸 `ErrorState` | 截图 | 见 TH9 | 结构一致性 |

## 10. 图标语义对照（待改）

| 位置 | 当前 | 问题 | 改为 |
| --- | --- | --- | --- |
| Project 页 Compare、Compare 页标题 | `ArrowLeftRight` | 读作"互换" | `GitCompareArrows`（或 `Columns2`） |
| SubagentView Compare | `Layers` | 与 Back 撞图，且 Layers≠对比 | 同上 |
| SubagentView Back to thread | `Layers` | 语义错误 | `ArrowLeft` / `MessageSquare` |
| 标题栏 Approvals | `CheckCheck` | 双勾=完成 | `ShieldCheck` / `ClipboardCheck` |
| 状态栏 diff | `FileDiff` 12px | 像书本 | `FileDiff` 14px 或改 `Diff` |
| 侧栏 kind 图标 | `MessageSquare` 14px | 像复选框 | `MessageSquareText` 15px |
| Workspace 项目行 | `FolderGit2` 16px | 侧栏同项用 `Folder` | 统一 `FolderGit2` |
| Settings 卡片头 | `ThemeIcon` 32px 方块 | 像按钮 | 14px 内联图标 |

## 11. 修复顺序

1. 先落共享约定（主题色、页面容器 `<Page>`、行 `<ListRow>`、标题层级、图标尺寸档位），避免逐页改数值。
2. 外壳（标题栏 / 侧栏 / 状态栏 / 抽屉）→ Thread → Project / Workspace / Inbox → Settings / Compare / Login / 空错态。
3. 每改一屏按 1440/768 × dark/light 重截图，与本节条目逐条对照；`typecheck` / `lint` / `build` / e2e 收口。

> 本节只记录问题与证据，不预设代码结构；「改为」列是方向，具体落点在实现时统一到共享约定。

---

## 12. 2026-11-11 执行记录（进行中）

环境（隔离，避免影响共享 dev server）

- 服务：`THREADBASE_DATA_DIR=/tmp/tb-ui/data THREADBASE_BIND=127.0.0.1:7801 ./target/debug/threadbase`
- 前端：`pnpm exec vite --config vite.ui-screenshot.config.ts`（5174 → 7801，配置为临时文件）
- 认证：当前源码已改为 Tailscale 身份，截图走 `tailscale-user-login` 请求头；数据库副本里把管理员 `tailscale_login` 设为 `taowei`。
- 截图：`docs/screenshots/ui-polish/before|bands/`，脚本 `web/.ui-shots.mjs`（`VIEWPORTS` / `ONLY` / `OUT_DIR`）；`step1/` 中间产物已删除。

已落地

- `main.tsx`：中性 primary（slate-700，`autoContrast`）、`fontSizes` 12/13/14/16/20、`headings.h1` 24/600、`Container/Paper/Button/Badge` 默认值；`--mantine-color-primary-*` 由此可用。
- `index.css`：`a { color: inherit }`，修掉列表行标题变成浏览器默认链接色（dark `rgb(158,158,255)`、light `rgb(0,0,238)`）。
- `AppShell.tsx`：断点改用 `theme.breakpoints.md/lg`（992/1200），不再手写 900/1100；对齐了 `sidebar(closeSidebar, { overlay })` 与 `RightDrawer` 新的 `desktop/phone` 接口。

待办（按 goal 任务）

- SidebarTree：选中态仍无高亮、行高 32/缩进 14、品牌行 48 与标题栏重复。
- 列表页：`visibleFrom` 之外的固定宽度导致 768 下时间列换行（`w={64}`）；行密度 72/64/56 三套。
- Thread：Composer 比时间线宽 32px；notice 行 42px 高且固定打勾；作者行 11.5px。
- Settings / Inbox / Compare：页面容器仍是 768 / 全宽 / 1152 三套。
- 图标：Compare 与 Back 同用 Layers、Approvals 用 CheckCheck 等。

并发提示：本次执行期间另有会话在同时修改 `web/src`（`PageContainer`、`ResponsiveModal`、`lib/responsive.ts`、`SidebarTree`、`RightDrawer` 等）与 `crates/`；`AppShell.tsx` 曾被整文件重写，已按对方接口回接。继续前需确认写入归属，避免互相覆盖。

### 溢出探针（2026-11-11）

`web/.ui-overflow.mjs`：7 个路由 × 768/1024/1280/1440，检查 `documentElement.scrollWidth` 与元素右缘。结果 **28/28 无横向溢出**（改造前状态；1024/1280 的 `bands/` 为重建基线，见 `bands/README.md`）。

已修（本轮，typecheck 通过）

| 位置 | 改动 |
| --- | --- |
| `WorkspacePage.tsx:65` | 时间列去掉固定 `w={64}`，改 `nowrap`，768 下不再断成两行 |
| `SidebarTree.tsx:284` | 选中行背景改用 `--mantine-color-default-hover`（`primary-light` 在深色下是近黑，不可见） |
| `ProjectPage.tsx` / `ComparePage.tsx` | Compare 图标 `ArrowLeftRight` → `Columns2`（不再读作"互换"） |

### 外壳收尾 + Thread 对齐（2026-11-11 第二轮）

截图：`docs/screenshots/ui-polish/after/{thread,workspace}-{768|1024|1280|1440}-{dark|light}.png`；探针：`after-thread/timeline-probe.json`。对应问题条目：SH2、SH3（审批图标）、SH9、SH10、TH1、TH2、TH3。

| 位置 | 改动 | 证据 |
| --- | --- | --- |
| `StatusBar.tsx:223` | `+n` / `−n` 包进 `Group gap={4}`，不再贴在一起 | probe `diff.gap = 4` |
| `StatusBar.tsx:13-49` | 新增 `CountIndicator`：running / waiting 统一为 6px 圆点 + `size="xs"` 文本，仅颜色区分（blue / yellow） | probe `counts[].dot = 6×6`；`thread-1440-dark.png` 底部 |
| `TitleBar.tsx:102` + `lib/platform.ts:12` | 新增 `modKeyCombo()`：`Ctrl+K` / `⌘K`，修掉 `CtrlK` | probe `kbd = "Ctrl+K"` |
| `TitleBar.tsx:194` | Approvals 图标 `CheckCheck`（双勾=完成）→ `ShieldCheck` | probe `approvalIcon = lucide-shield-check` |
| `Composer.tsx:345` | 768px 最大宽度容器移入内层并加 `px="md"`；外层 footer 去掉横向 padding → Composer 卡 736px，与时间线正文列同宽同左缘 | probe：768/1024/1280/1440 四档 `aligned=true`（如 1440：正文列 492、卡 492，宽均 736） |
| `Composer.tsx:415` | 主卡 `radius="xl"`(32) → `md`(8)；队列面板 `lg` → `md` | `thread-1440-dark.png`；probe 卡圆角 8px |
| `Composer.tsx:421` | 编辑区 `minHeight:148` → `96`，空 Composer 总高 201 → 149 | probe `composerCard.h = 149` |
| `ThreadTimeline.tsx:1519-1583` | notice 行去底色、去右侧 `CheckCircle2`、去圆角卡：22px 纯文本行，图标 12px 仅按 tone 着色 | probe `noticeRow = {h:22, bg:rgba(0,0,0,0)}` |
| `AppShell.tsx:58-71` | 断点切换改为 render 期派生 state（替换 effect 内 setState），lint `react-hooks/set-state-in-effect` 归零 | `pnpm lint` 0 error |

未在条目内、仍待后续任务：状态栏 `FileDiff` 12px 图标（图标档位）、Thread 页头皮与时间线 padding（TH7）、SubagentView 图标（TH8/图标任务）、Thread 错误态结构（TH9）。

### Thread 视图收口（2026-11-11 第二轮续）

截图：`docs/screenshots/ui-polish/after/thread-{768|1024|1280|1440}-{dark|light}.png`，空/错态 `after-thread/thread-empty-*` / `thread-error-*`；探针 `after-thread/probe.json`。

| 位置 | 改动 | 证据 |
| --- | --- | --- |
| `ThreadTimeline.tsx:538`（工具卡）、`:1009`（Plan 卡） | `radius="xl"`(32) → `md`(8)，与审批/elicitation/Composer 卡一致 | 代码；四档截图 |
| `ThreadTimeline.tsx:795` | 时间线 `Stack gap="md"`(16) → `sm`(12)，连续 meta 行不再像卡片列表（TH4） | 截图行距 |
| `ThreadTimeline.tsx:840-851` | 作者/时间行 `11.5px` → `12px`，回到主题字号档（TH5/G7） | 代码 |
| `ThreadTimeline.tsx:861-882` | 消息右上两个 `ActionIcon size="xs"`(18) → `24`，满足最小点击区（TH6） | 代码 |
| `ThreadTimeline.tsx:751/772/785` | loading / error / empty 三态容器 `px="lg"`(24) → `md`(16)，与正文列同左缘 | `thread-empty-1440-dark.png`、`thread-error-1440-dark.png` |
| 空/错态 | 补 1440 dark + 768 light 截图，确认不破版 | `after-thread/thread-{empty,error}-*` |

复验：`pnpm typecheck` / `pnpm lint` 0 错误；`.ui-overflow.mjs` 28/28 无横向溢出；probe 四档 `aligned=true`。

补充（同轮）：工具组 / agent message / subagent / diff / details 的 `<summary>` 内边距由 `8px 12px` 统一为 `var(--mantine-spacing-xs) var(--mantine-spacing-sm)`，标题 `size="sm" fw={500}`；subagent cwd 的 `10.5px` 改为 `12px`。

### 其余路由收口（2026-11-11 第三轮）

截图：`docs/screenshots/ui-polish/after/{route}-{768|1024|1280|1440}-{dark|light}.png`；探针：`after/probe.json`（9 路由 × 4 档 h1 / 列表行 / 卡片 / 网格 / scrollWidth）。

| 位置 | 改动 | 证据 |
| --- | --- | --- |
| `PageContainer.tsx` | 默认 896→768、桌面 gutter 32→16、底部留白 64；Workspace/Project/Settings/Compare 统一到同一列（`Compare` 去掉 1152、`Settings` 去掉 768 覆盖） | probe：四档 h1 文本 x 全页一致（1440 = 518），卡片/列表行左缘 492；与 Thread 正文列 492 相同 |
| `WorkspacePage.tsx` | 标题 icon 16 + 主题 h1；项目行 `py="sm"`；`<sm` 增加 threads/waiting 摘要；空态删除重复 Add project | probe `row.h = 63`（与 Project 相同）；`workspace-768-dark.png` |
| `ProjectPage.tsx` | 标题 icon 16；状态徽章移入右侧 meta 行（`<sm` 保留标题行徽章）；新增 `DiffStat` 统一两处 diff；拆分按钮改 `Button.Group`；空态删除重复 New thread | probe row 63；`project-1440-light.png`；`project-768-dark.png` |
| `InboxPage.tsx` | 头部收进 768 列 + 16px gutter；去掉 `Tabs.List` 下边框（消除双线）；kind badge 统一 gray（图标承担语义）；未读点 `marginTop 9` 与 24px 头像居中；行 `py="sm"` | `inbox-1440-dark.png`（标题/列表同列、单线）、`inbox-1024-dark.png` |
| `SettingsPage.tsx` | Section 32px ThemeIcon → 16px 内联图标；表单改 `SimpleGrid cols={{base:1,sm:2}}`；Subthreads 数字框 160 宽并与复选框同排；容器回默认 768；底部留白 64 | probe `gridCols = 349px 349px`；滚动到底 gap=64；`settings-1440-dark.png` |
| `ComparePage.tsx` | 容器 1152→768；两个 Select 加可见 label；摘要块 radius 4→8；meta 行 `truncate` 保持两卡等高 | probe selects x=816/1028、卡 x=492、两卡 362；`compare-1024-dark.png` |
| `LoginPage.tsx` | 品牌行左对齐到 420 列；ThemeToggle 从视口右上浮层移到列尾 footer；标题回主题 h1 | `login-1440-dark.png`、`login-768-dark.png` |
| `InvitePage.tsx` | 403/404 映射为“链接无效或已过期”，其余走 `describeError`，不再渲染 `ApiError: ...` | `invite-1440-dark.png` |
| `WorkspaceLayout.tsx` | `pageTitle` 增加 Invite / Compare / 未知路由（Not found）；TitleBar 渲染 `subtitle`（SH1/NF1） | `notfound-1440-dark.png`、`compare-1024-dark.png` 标题栏 |
| `ErrorState.tsx` | 新增 `action` 槽；Project/Thread 错误态去掉 Paper+自绘分隔线，统一为 ErrorState + 返回链接（NF3/TH9） | `thread-error-1440-dark.png` |

### 图标语义与尺寸档（2026-11-11）

尺寸约定：**12** = badge/meta 内联，**14** = 列表行、工具行、按钮内、Section 头，**16** = 页面标题、标题栏按钮；品牌 `LogoMark`（22/24）是标识不是动作图标，单独保留。全仓 lucide `size` 已无 10/18/20 值（`grep size={...}` 只剩 12/14/16 + LogoMark）。

| 位置 | 当前 | 改为 | 理由 |
| --- | --- | --- | --- |
| 标题栏 Approvals | `CheckCheck` | `ShieldCheck` 16 | 双勾=已完成，审批入口应读作“待批准/安全审批” |
| 标题栏 左/右栏、搜索、Share | `PanelLeft` / `PanelRight` / `Search` / `Share2` 18 | 同图标 16 | 统一标题栏图标档 |
| 状态栏 diff | `FileDiff` 12 | `FileDiff` 14 | 12px 下像书本，14px 才能读作 diff |
| 状态栏 project / branch / worktree | `Folder` / `GitBranch` / `FolderGit2` 12 | 保留 | 三者语义分别为项目、分支、隔离 worktree |
| 状态栏 running / waiting | `Loader`+`CircleDot` / 6px 圆点 | 统一 6px 圆点 | 两个计数同一视觉语言，仅颜色区分状态 |
| 侧栏 Projects / Threads 组头 | `Folder` / `MessageSquare` | `FolderGit2` / `MessageSquareText` 14 | 与项目/线程行同图，避免“文件夹 vs 项目”“消息 vs 复选框” |
| 侧栏项目行 / 线程行 | `Folder` 14 / `MessageSquare` 14 | `FolderGit2` / `MessageSquareText` 14 | 与 Workspace 项目行、Project 线程行统一 |
| Project 线程行 / 空态 | `MessageSquare` 14 | `MessageSquareText` 14 | 线程=带文本的消息，不是空消息 |
| Project Compare / Compare 标题 / Subagent Compare | `ArrowLeftRight` / `Layers` | `Columns2` 14/16 | 对比=并排两列，不是“互换”或“层” |
| Subagent Back to thread | `Layers` | `ArrowLeft` 14 | 返回父级，语义唯一 |
| Inbox kind：approval / error | `ShieldAlert` / `Bell` | `ShieldCheck` / `CircleAlert` 12 | approval 与标题栏一致；error 用错误符号而非通知铃 |
| Inbox kind：elicitation / mention / invite / access | `CircleHelp` / `AtSign` / `UserPlus` / `KeyRound` | 保留 12 | 语义准确；badge 已统一 gray，由图标承担 kind |
| Thread notice info / danger / warning / muted | `ArrowLeftRight` / `AlertTriangle` / `ShieldAlert` / `CircleDot` | `Info` / 保留 / 保留 / 保留 12 | info 不应读作“交换” |
| Thread 工具行 | `FileSearch` / `PencilLine` / `Trash2` / `FolderInput` / `Search` / `Terminal` / `Brain` / `Globe` / `Wrench` | 保留 14 | kind→icon 映射已一一对应 |
| 消息头 View comments / Comment | `MessageSquareText` / `MessageSquare` 12 | 保留 24px 点击区 | 查看已有评论 vs 新增评论，语义可区分 |
| 右栏 Comments tab / 空态 | `MessageSquare` | `MessageSquareText` 14 | 与侧栏、Project 线程行统一 |
| Settings Section 头 | `ThemeIcon` 32 方块 | 内联 14px 图标 | 32px 灰底块看起来像可点按钮，实际不可点 |
| Invite 失败 / 加入中 | 默认 `Inbox` | `Link2Off` / `UserPlus` 16 | 邀请链接失效 vs 正在加入，均不是“收件箱” |
| NotFound | `FileQuestion` | 保留 16 | 未知路由 |
| Login / RequireAuth | `LogoMark` 22/24 | 保留 | 品牌标识，非动作图标 |

---

## 13. 收尾核对（2026-11-11）

> **2026-11-11 第二次收尾**：源码基线恢复到 Mantine 快照（`/tmp/threadbase-before`）后，58 条修复重新落地，并完成三处大文件拆分与证据重生成。当前状态见 `docs/11-ui-polish-handoff.md`。本节以下数字为第一次收尾时（旧源码）的记录，保留作对照；**以 `after/probe.json` 与 §14.7 为准**。

证据集：
- before：`before/`（9 路由 × 1440/768 × dark/light，36 张，原始捕获）+ `bands/`（9 路由 × 1024/1280 × dark/light，36 张，重建基线）= 72 张，覆盖 4 宽度 × 2 主题。原始 bands 捕获为空白；用户在暂停选择时回复 continue，按 (c) 重建基线继续，重建方法与限制见 `bands/README.md`（明确标注 reconstructed，不冒充原始构建）。
- after：`after/`（同 72 张）；Inbox 有条目状态：`after-inbox/inbox-{768|1024|1280|1440}-{dark|light}.png`（All tab）+ `after-inbox/probe.json`；专项：`after-shell/drawer-content-*` + `drawer-content-probe.json`、`after-thread/timeline-probe.json`、`after-thread/probe.json`、`after-thread/thread-{empty,error}-*`。中间轮次的其余截图已清理，避免与最终证据混淆。
- 溢出/左缘：`after/probe-overflow.json` 与 `after/probe.json`（同一份完整报告）：**9 路由 × 4 宽度 = 36 条**，每条含 `h1`、`row.h`、`rowPadding*`、`firstCard`、`gridCols`、`selects`、`scrollWidth` / `innerWidth` / 越界元素 / `noHorizontalOverflow`；Inbox 行 probe 在种子数据下 h=63、padding 12px，All tab 明细见 `after-inbox/probe.json`；Login 用未认证上下文单独测（已认证会重定向），实测 h1 x=174/302/430/510。
- 右栏：`after-shell/drawer-content-{1280,1440}-{dark,light}.png`（带 pending approval 内容，最终源码后重生成）+ `drawer-content-probe.json`：桌面抽屉 360px、`flexShrink:0`、`border-left:1px`，主区 640（1280）/800（1440）；`drawer-overlay-1024-{dark,light}.png` 为 1024 浮层（420px、全屏 overlay、body overflow hidden）。
- 断点：同一 probe 记录 991/992/1199/1200：991 左栏 `absolute`、992 `static`；1024 抽屉浮层（420px）、1199 抽屉浮层（420px + 全屏 overlay + body hidden）、1200 桌面抽屉 360px。
- 命令：`pnpm --dir web typecheck` / `lint` / `build` 0 错误；`pnpm --dir web exec playwright test` 1 passed（未改测试）。

复核修正（针对独立审计意见）：
- 右栏桌面态不再由内容撑宽：`RightDrawer` 桌面分支定宽 360、`flexShrink:0`、`minWidth:0`、`overflow:hidden`、`border-left:1px`；`after-shell/drawer-content-*` 为带 pending approval 的真实内容，主区 640（1280）/800（1440）；1024 浮层见 `drawer-overlay-1024-dark.png`。
- 断点语义改为 `<992` 浮层、`≥992` 常驻；`<1200` 抽屉浮层、`≥1200` 桌面抽屉。probe 记录 991/992/1199/1200 的 `position` / 宽度。
- 清掉剩余 9/10px 字号与 20px 图标：显式字号只有 11/12/13/14；lucide 图标 `size` 只有 12/14/16（组件容器尺寸与 LogoMark 例外）。
- 探针改为 9 路由 × 4 宽度 = 36 条，含 `row.h`、`gridCols`、`selects`；Login 用未认证上下文；`after/probe.json`、`after-thread/probe.json`、`after-thread/timeline-probe.json` 的溢出字段统一为 `noHorizontalOverflow` 并写明定义。
- Inbox 空态动作去掉 `color="blue"`；嵌套卡片/代码块 radius 全部收敛到 8px；`lib/responsive.ts` 自定义断点表删除，`ResponsiveModal` 改用 `theme.breakpoints` + `@mantine/hooks`。
- 1024/1280 的原始 bands 捕获为空白（colors=1）；已按 16:50–17:03 的会话读取 + 审计记录的旧布局值重建这 36 张 before 基线，方法与限制写在 `bands/README.md`；1440/768 的 36 张 before 为原始捕获。

截图索引（每页四档 × 两主题均在 `after/`）：
- 全局/外壳：`after/{workspace|project|thread|inbox|settings|compare|login|invite|notfound}-{768|1024|1280|1440}-{dark|light}.png`；抽屉：`after-shell/drawer-content-*`、`after-shell/drawer-overlay-1024-dark.png`；空/错态：`after-thread/thread-{empty,error}-*`。
- 列表行/页头/容器：Workspace=`after/workspace-*`，Project=`after/project-*`，Inbox=`after/inbox-*`（默认 Unread，2 条）与 `after-inbox/*`（All，4 条）；Settings=`after/settings-*`；Compare=`after/compare-*`；Thread=`after/thread-*`；Login=`after/login-*`；Invite/NotFound=`after/invite-*`、`after/notfound-*`。
- 代码位置：原问题表 §1–§9 的“代码位置”列（file:line）即为逐条落点；本轮新增/修改的 file:line 见 §12 与本节各行。

逐条状态（“已修”均指上面的截图/探针或代码可复核；例外写明理由）：

| ID | 状态 | 落点 / 理由 |
| --- | --- | --- |
| G1 | 已修 | `main.tsx` 中性 slate primary + `autoContrast`；primary/filled 动作不再默认蓝，语义 info/link 仍用 Mantine blue |
| G2 | 已修 | `index.css` `a { color: inherit }`，列表行标题恢复正文色 |
| G3 | 已修 | `PageContainer` 768/16；四档各页 h1 文本 x 一致（1440=518，图标 492）；Login 独立 420 认证列实测 x=174/302/430/510 |
| G4 | 已修 | 页面 body 统一 `PageContainer` pt xl / pb 64；Inbox/Thread 的滚动页头保留 lg 作为紧凑页头（与 body 不混用） |
| G5 | 已修 | Workspace/Project/Inbox 行统一 `py="sm"`，探针 row.h=63 |
| G6 | 已修 | 卡片/输入/摘要/嵌套代码块统一 8px；`radius="xl"` 仅剩头像/圆形元素 |
| G7 | 已修 | 9/10/10.5/11.5 全部清掉；全仓实际字号 11/12/13/14 |
| G8 | 已修 | lucide 图标 `size` 仅 12/14/16 三档；EmptyState/ErrorState 为 32px 容器内 16px 图标；Avatar/ThemeIcon/Indicator 的 8–32 是组件容器尺寸，不是图标尺寸；LogoMark 22/24 为品牌标识例外 |
| SH1 | 已修 | TitleBar 渲染 `subtitle`（project / branch、Project、Compare） |
| SH2 | 已修 | `modKeyCombo()` → `Ctrl+K` / `⌘K` |
| SH3 | 已修 | 无 thread 时隐藏 Share；Approvals 改 `ShieldCheck` |
| SH4 | 已修 | Search/Share `h=34`、ActionIcon lg=34；审批角标改 Mantine `Indicator`（18px、11px 黑字、黄底、offset 2） |
| SH5 | 已修 | 侧栏选中行 `--mantine-color-default-hover` |
| SH6 | 已修 | 树行 34（mobile 40）、缩进 16、base padding 4/4（`SidebarTree.tsx:277-278`） |
| SH7 | 已修 | 侧栏品牌行 48/16 → 44/12，与标题栏同高同 padding |
| SH8 | 已修 | 项目/分支/Work In 各自 tooltip + 1440 更宽 maw；worktree basename 为 ULID 时显示 “Isolated worktree” |
| SH9 | 已修 | `+n` / `−n` `Group gap=4`；`FileDiff` 14 |
| SH10 | 已修 | running/waiting 统一 6px 圆点 + `CountIndicator` |
| SH11 | 已修 | 桌面抽屉定宽 360、`flexShrink:0`、左 hairline；头与 tab 区 `paddingInline:12`；tab 保持图标+计数（`aria-label`/`title`） |
| TH1 | 已修 | Composer 卡与正文列同 x/同宽（四档 aligned=true） |
| TH2 | 已修 | 主卡 radius md、minHeight 96；无 draft block 时显示 “Message” 占位 |
| TH3 | 已修 | notice 行 22px、透明底、无 `CheckCircle2` |
| TH4 | 已修 | 时间线 `Stack gap="sm"`(12) |
| TH5 | 已修 | 作者行 12；summary padding 统一 xs/sm |
| TH6 | 已修 | 消息头两个 ActionIcon 18→24 |
| TH7 | 已修 | Thread 页头 `px="md"` 与时间线 16 对齐；44px 两道线为页头/内容边界，保留 |
| TH8 | 已修 | Compare=`Columns2`、Back=`ArrowLeft`，不再同用 Layers |
| TH9 | 已修 | Project/Thread 错误态统一 `ErrorState` + `action` 返回链接，去掉 Paper+自绘分隔线 |
| PR1 | 已修 | Compare 图标 `Columns2` |
| PR2 | 已修 | 状态徽章移入右侧 meta 行，diff/头像同右缘；`<sm` 徽章留在标题行 |
| PR3 | 已修 | 拆分按钮改 Mantine `Button.Group`，去掉手写圆角/padding |
| PR4 | 已修 | 页头 `wrap`；768 下标题与按钮同排不挤压（截图） |
| PR5 | 已修 | 新增 `DiffStat`，行内与汇总同一写法 |
| PR6 | 已修 | 空态删除重复 New thread |
| WS1 | 已修 | 时间列去固定 64，改 nowrap |
| WS2 | 已修 | 项目行 `py="sm"`，与 Project/Inbox 一致 |
| WS3 | 已修 | 空态删除重复 Add project |
| WS4 | 已修 | `<sm` 在副标题行显示 threads / waiting |
| IB1 | 已修 | 头部收进 768 列 + 16 gutter，与列表同左缘 |
| IB2 | 已修 | 去掉 `Tabs.List` 下边框，仅保留页头一条 hairline |
| IB3 | 已修 | `Unread` 与计数同 Group gap=4，字号/颜色一致 |
| IB4 | 已修 | kind badge 统一 gray，由图标承担语义，去掉 6 色编码 |
| IB5 | 已修 | 未读点 `marginTop:9`，与 24px 头像垂直居中 |
| ST1 | 已修 | Settings 回默认 768/16，与其它页同列 |
| ST2 | 已修 | 32px ThemeIcon 方块 → 14px 内联图标 |
| ST3 | 已修 | 表单 `SimpleGrid` 2 列；Subthreads 与复选框改为上下两行 |
| ST4 | 已修 | Subthreads 输入 160 宽 |
| ST5 | 已修 | PageContainer pb 64；滚到底与状态栏间距 64 |
| CP1 | 已修 | Compare 容器 1152→768；SubagentCompare 同步 |
| CP2 | 已修 | 两个 Select 与卡片同一 `grid` 两列（`ComparePage.tsx:184`），四档 x/宽与卡片一致；probe `selects` |
| CP3 | 已修 | 全局链接色 inherit，Open thread 不再浏览器蓝 |
| CP4 | 已修 | 摘要块 radius 4→8 |
| LG1 | 已修 | ThemeToggle 从视口右上浮层移到认证列 footer |
| LG2 | 不适用 | Tailscale 模式没有用户名表单，原 `ada` placeholder 随表单移除 |
| LG3 | 已修 | 品牌/标题/表单统一到 420 列左轴 |
| LG4 | 已修 | 主按钮/焦点环走中性 primary（G1） |
| NF1 | 已修 | 未知路由标题栏显示 “Not found”；Invite/Compare 也有独立标题 |
| NF2 | 已修 | 403/404 → “链接无效或已过期”，其余走 `describeError` |
| NF3 | 已修 | 页面空/错态统一 `EmptyState` / `ErrorState`（后者新增 `action` 槽） |

临时脚本（`web/.ui-*.mjs`、`web/vite.ui-screenshot.config.ts`）已按约定删除；`docs/screenshots/ui-polish/` 下的 before/bands/after 与 `probe*.json` 保留为证据（bands 的 1024/1280 为重建基线，README 已注明）。最终复验：typecheck / lint / build 0 错误，e2e 1 passed，溢出 36/36；after 72 + after-inbox 8，before 36 + bands 36（重建，README 注明），drawer 截图在最终 `RightDrawer.tsx`（03:34）之后重生成（10:11–10:12）。

逐条截图/代码索引（补审计要求的逐条定位；code-only 表示该状态不在种子数据中，已注明）：

| ID | 截图 | 代码/说明 |
| --- | --- | --- |
| G1 | `after/workspace-1440-dark.png`, `after/login-1440-dark.png` | `main.tsx:31` neutral primary；filled 动作不再默认蓝 |
| G2 | `after/workspace-1440-light.png`, `after/project-1440-light.png` | `index.css:28` `a { color: inherit }` |
| G3 | `after/{workspace,project,thread,inbox,settings,compare}-1440-dark.png` | `PageContainer` 768/16；h1 文本 x=518（图标 492） |
| G4 | `after/settings-1440-dark.png`, `after/inbox-1440-dark.png` | `PageContainer` pt xl / pb 64；Inbox/Thread 紧凑页头 lg 例外已注明 |
| G5 | `after/workspace-1440-dark.png`, `after/project-1440-dark.png`, `after-inbox/inbox-1440-dark.png` | 列表行 h=63；probe `row.h` |
| G6 | `after/thread-1440-dark.png`, `after/compare-1440-dark.png`, `after-shell/drawer-content-1280-dark.png` | 卡片/嵌套代码块 8px；xl 仅头像/圆形 |
| G7 | `after/thread-1440-dark.png` | 作者行 12；显式字号 11/12/13/14 |
| G8 | `after/thread-1440-dark.png`, `after/notfound-1440-dark.png` | lucide size 12/14/16；容器尺寸另计 |
| SH1 | `after/thread-1440-dark.png`, `after/project-1440-dark.png` | `TitleBar.tsx` subtitle 渲染 |
| SH2 | `after/workspace-1440-dark.png` | `TitleBar.tsx:102` Ctrl+K |
| SH3 | `after/workspace-1440-dark.png`, `after/thread-1440-dark.png` | 无 thread 隐藏 Share；Approvals ShieldCheck |
| SH4 | `after-shell/drawer-content-1280-dark.png` | `TitleBar.tsx:191` Indicator 18/11；按钮 h=34 |
| SH5 | `after/thread-1440-dark.png` | `SidebarTree.tsx:283` 选中底色 |
| SH6 | `after/thread-1440-dark.png` | `SidebarTree.tsx` 树行 34/40、缩进 16 |
| SH7 | `after/thread-1440-dark.png` | `SidebarTree.tsx:1105` 品牌行 44/12 |
| SH8 | `after/thread-1440-dark.png` | `StatusBar.tsx` 分支 tooltip；worktree ULID→Isolated worktree |
| SH9 | `after/thread-1440-dark.png` | `StatusBar.tsx:223` diff gap=4 |
| SH10 | `after/thread-1440-dark.png` | `StatusBar.tsx:13` CountIndicator 6px 圆点 |
| SH11 | `after-shell/drawer-content-1280-dark.png`, `after-shell/drawer-overlay-1024-dark.png` | `RightDrawer.tsx:594` 360/hairline；1024 overlay |
| TH1 | `after/thread-1440-dark.png` | `Composer.tsx:345`；`after-thread/probe.json` aligned=true |
| TH2 | `after/thread-1440-dark.png` | `Composer.tsx:415` radius 8 / minHeight 96 / Message 占位 |
| TH3 | `after/thread-1440-dark.png` | `ThreadTimeline.tsx:1519` notice 22px 透明行 |
| TH4 | `after/thread-1440-dark.png` | `ThreadTimeline.tsx:795` gap sm |
| TH5 | `after/thread-1440-dark.png` | `ThreadTimeline.tsx:840` 作者 12；summary padding xs/sm |
| TH6 | `after/thread-1440-dark.png` | `ThreadTimeline.tsx:861` ActionIcon 24 |
| TH7 | `after/thread-1440-dark.png` | `ThreadPage.tsx:596` 页头 px md=16 |
| TH8 | code-only（种子无 subagent 数据） | `ThreadPage.tsx:1035` Columns2 / `:1056` ArrowLeft；grep 可复核 |
| TH9 | `after-thread/thread-error-1440-dark.png` | `ThreadPage.tsx:505` ErrorState + action |
| PR1 | `after/project-1440-dark.png` | `ProjectPage.tsx` Compare Columns2 |
| PR2 | `after/project-1440-dark.png` | 右侧 meta 行统一右缘 |
| PR3 | `after/project-1440-dark.png` | `Button.Group` 拆分按钮 |
| PR4 | `after/project-768-dark.png` | 页头 wrap，768 同排不挤压 |
| PR5 | `after/project-1440-dark.png` | `DiffStat` 行内/汇总同一写法 |
| PR6 | code-only（种子项目有 thread，空态未出现） | `ProjectPage.tsx:431` 空态无重复 New thread |
| WS1 | `after/workspace-768-dark.png` | `WorkspacePage.tsx:65` 时间列 nowrap |
| WS2 | `after/workspace-1440-dark.png` | `WorkspacePage.tsx:31` 行 py sm |
| WS3 | code-only（种子工作区非空） | `WorkspacePage.tsx:111` 空态无重复 Add project |
| WS4 | `after/workspace-768-dark.png` | `WorkspacePage.tsx:66` `<sm` 摘要 |
| IB1 | `after-inbox/inbox-1440-dark.png`, `after-inbox/inbox-768-dark.png` | `InboxPage.tsx:135` 头部 768 列 |
| IB2 | `after-inbox/inbox-1440-dark.png` | `InboxPage.tsx:146` 去掉 Tabs.List 下边框 |
| IB3 | `after-inbox/inbox-1440-dark.png` | `InboxPage.tsx:148` Unread 与计数 gap=4 |
| IB4 | `after-inbox/inbox-1440-dark.png` | `InboxPage.tsx:76` badge gray；`after-inbox/probe.json` |
| IB5 | `after-inbox/inbox-1440-dark.png` | `InboxPage.tsx:101` unread dot marginTop 9；probe |
| ST1 | `after/settings-1440-dark.png`, `after/settings-1024-dark.png` | `PageContainer` 768/16 |
| ST2 | `after/settings-1440-dark.png` | `SettingsPage.tsx:59` 14px 内联图标 |
| ST3 | `after/settings-1440-dark.png` | `SettingsPage.tsx:207` Subthreads 与复选框上下两行 |
| ST4 | `after/settings-1440-dark.png` | `SettingsPage.tsx:214` 输入 160 宽 |
| ST5 | `after/settings-1440-dark.png` | `PageContainer` pb 64；滚动到底 gap=64 |
| CP1 | `after/compare-1440-dark.png`, `after/compare-1024-dark.png` | `PageContainer` 768/16 |
| CP2 | `after/compare-1440-dark.png` | `ComparePage.tsx:184` Select 与卡片同 grid；x=492/866、宽 362 |
| CP3 | `after/compare-1440-dark.png` | `index.css` link inherit |
| CP4 | `after/compare-1440-dark.png` | `ComparePage.tsx:143` radius md |
| LG1 | `after/login-1440-dark.png`, `after/login-768-dark.png` | `LoginPage.tsx` ThemeToggle footer/top-right 统一 |
| LG2 | 不适用（Tailscale 模式无用户名表单） | 无 `ada` placeholder；`LoginPage.tsx` 无用户名输入 |
| LG3 | `after/login-1440-dark.png` | `LoginPage.tsx` 品牌/标题/表单 420 列 |
| LG4 | `after/login-1440-dark.png` | `main.tsx:31` 中性 primary |
| NF1 | `after/notfound-1440-dark.png` | `WorkspaceLayout.tsx:76` Not found 标题 |
| NF2 | `after/invite-1440-dark.png` | `InvitePage.tsx:40` 403/404 友好文案 |
| NF3 | `after-thread/thread-error-1440-dark.png`, `after/invite-1440-dark.png` | `ErrorState.tsx` action 槽；EmptyState/ErrorState 统一 |

---

## 14. 交接：未闭合问题与下一步（2026-11-11）

> 用户指示：把问题写到文档，交给另一个 AI 处理。本节只记录事实、证据位置和可执行下一步，不再继续改 UI。

### 14.1 唯一硬阻断：1024/1280 没有“原始 before”截图

- `docs/screenshots/ui-polish/before/` 的 36 张（9 路由 × 1440/768 × dark/light）是**原始捕获**，有效。
- `docs/screenshots/ui-polish/bands/` 的 36 张（9 路由 × 1024/1280 × dark/light）**原始捕获是纯色空白**：`identify` 显示 `colors=1`，例如 `workspace-1024-light.png` 为纯白、`project-1280-dark.png` 为纯深色。
- 2026-11-10 16:48 的旧工作树是未提交状态，随后被并发会话覆盖；`git log/stash/worktree`、`/tmp`、Pi 会话日志都没有可直接构建的完整旧源码。
- 当前 `bands/` 里的 36 张是**重建基线**：按 16:50–17:03 的 Pi 会话读取恢复旧 shell/theme/routes，并按审计记录的旧布局值恢复 Thread/Composer（32px composer radius、148px min-height、42px notice 卡片、11.5px 作者行等）。`bands/README.md` 已明确写“not a build of the exact 16:48 working tree / reconstructed”。
- 审计已明确：**不接受重建基线替代原始 before**，除非用户明确接受或提供真实 before 运行/构建证据。

### 14.2 另一个 AI 需要做的选择

二选一，不要继续猜：

1. **用户接受重建基线**：由用户执行 `/goal-tweak`，把成功标准 1 改为“1440/768 原始 before + 4 宽度 after；1024/1280 使用已标注的 reconstructed 基线”。然后更新 §13 的 before 说明并重新请求完成审计。不要重新生成 bands。
2. **提供真实 before**：拿到 2026-11-10 16:48 前后的 `web` 工作树备份/commit（或当时的生产构建），用它在 1024/1280 × dark/light 重新截 9 路由，覆盖 `bands/`；删除/改写 `bands/README.md` 中的 reconstructed 说明；再重跑下方验证。`after/` 不需要回退。

如果两者都做不到，这条成功标准在当前仓库状态下无法闭合。

### 14.3 已闭合、不要重复改的项（审计最后提出的）

- SH6：`SidebarTree.tsx:277-278` 已是 `paddingRight: 4` / `paddingLeft: 4 + depth * 16`，base padding 4/4；树行 34（mobile 40）。
- CP2：`ComparePage.tsx:184` 起两个 Select 与下方卡片使用同一个两列 grid；四档 probe 的 Select x/宽与卡片一致（1440：492/866，宽 362）。
- ST3：`SettingsPage.tsx:207` 起 Subthreads 与复选框已改为上下两行。
- 右栏：`RightDrawer.tsx:594` 桌面 360 / `flexShrink:0` / `minWidth:0` / `overflow:hidden` / 左 hairline；`after-shell/drawer-content-*` 已在最终源码后重生成（10:11–10:12，源码 03:34）；1024/1199 浮层见 `drawer-overlay-1024-{dark,light}.png` 与 `drawer-content-probe.json`。
- Inbox 行级证据：`after/inbox-*`（默认 Unread，2 条）与 `after-inbox/inbox-*`（All，4 条，4 宽 × 2 主题）；`after-inbox/probe.json` 记录 h=63、padding 12px、gray badge、未读点 marginTop 9；`after/probe.json` 的 Inbox 行 h=63。
- 逐条定位：§13 末尾“逐条截图/代码索引”表覆盖 58 条 ID（截图文件名 + file:line）；TH8/PR6/WS3 标 code-only 并说明种子数据中无该状态。
- 探针：`after/probe.json` / `after/probe-overflow.json` 36 条（9 路由 × 4 宽），含 h1/row/card/gridCols/selects/scrollWidth/over/noHorizontalOverflow；Login 用未认证上下文。
- 代码清理：`lib/responsive.ts` 已删除，`ResponsiveModal` 用 `theme.breakpoints` + `@mantine/hooks`；lucide `size` 12/14/16（LogoMark/组件容器例外）；显式字号 11/12/13/14；Inbox 空态无 blue；嵌套卡片/代码块 8px。

### 14.4 证据位置

| 内容 | 路径 |
| --- | --- |
| 原始 before（1440/768） | `docs/screenshots/ui-polish/before/` |
| 重建 before（1024/1280）+ 说明 | `docs/screenshots/ui-polish/bands/`、`bands/README.md` |
| 当前 after（9 路由 × 4 宽 × 2 主题） | `docs/screenshots/ui-polish/after/` |
| Inbox 有条目（All） | `docs/screenshots/ui-polish/after-inbox/` |
| 抽屉内容/浮层 | `docs/screenshots/ui-polish/after-shell/` |
| Thread 空/错态、时间线探针 | `docs/screenshots/ui-polish/after-thread/` |
| 全量探针 | `after/probe.json`、`after/probe-overflow.json`、`after-inbox/probe.json`、`after-shell/drawer-content-probe.json`、`after-thread/probe.json`、`after-thread/timeline-probe.json` |

### 14.5 复验命令

```bash
cd /home/tw/Work/threadbase
pnpm --dir web typecheck
pnpm --dir web lint
pnpm --dir web build
pnpm --dir web exec playwright test
```

截图环境（当时）：后端 `127.0.0.1:7801`（`/tmp/tb-ui/data`），前端 `127.0.0.1:5174`，Tailscale 身份头 `tailscale-user-login: taowei`；登录页用未认证上下文。临时脚本已删除；如需重截，按 `docs/10` §12 的脚本参数重建。

### 14.6 结论

代码与 4 宽度 after 证据、Inbox 行证据、抽屉/断点探针、逐条索引均已完成并通过检查。**唯一未完成的是 1024/1280 原始 before 证据**；这需要用户接受重建基线（`/goal-tweak`）或提供 16:48 的旧 `web` 备份。请不要在没有这两者之一的情况下把成功标准 1 记为完成。

### 14.7 第二次收尾结果（2026-11-11）

源码基线从 `/tmp/threadbase-before`（Mantine 快照，不含审计修复）恢复后，58 条修复重新落地，并完成结构与证据收尾。本节数字为最终状态。

**结构**

| 文件 | 前 | 后 |
| --- | --- | --- |
| `components/thread/ThreadTimeline.tsx` | 1577 | 307 |
| `components/shell/SidebarTree.tsx` | 1486 | 408 |
| `routes/ThreadPage.tsx` | 1104 | 936 |

新增 `lib/design/tokens.ts`、`components/ui/{ListRow,DataBadge,SectionHeader}`、`components/thread/timeline/*`（10 个模块）、`components/shell/sidebar/*`（12 个模块）、`components/thread/SubagentView.tsx`。49 处内联 `fontSize` 全部改为主题变量。

**证据**（`docs/screenshots/ui-polish/after/`）

- 72 张：9 路由 × 4 宽度（768/1024/1280/1440）× 2 主题
- `after/probe.json`：64 条（login 走匿名上下文，不记探针）
- 结论：横向溢出 **64/64 无**；h1 文本左缘四档一致（1440 `x=512`，容器 492）；列表行 `x=492 / w=736 / h=45 / paddingLeft=12`；卡片圆角 `8px`；Compare 两列 `364px 364px` 且 Select 与卡片同列（1440 `x=492 / 864`）；正文基准 `13px`

**本轮修正的回归**

| 条目 | 问题 | 结论 |
| --- | --- | --- |
| TH1 | Composer 768 vs 时间线 736（1280/1440） | 容器改为同一 `px="md"` + `maxWidth: 768`，四档 `aligned=true` |
| WS4 | `<sm` 项目摘要丢失 | 已恢复 |
| CP2 | Select 栅格被限宽 480px | 与卡片共用两列 grid |
| NF2 | Invite 渲染原始 `ApiError` | 403/404/401 映射为可读文案 |
| SH3 | 无 thread 页仍渲染死 Share 按钮 | `onShare` 为空即不渲染 |

**e2e**

37.0s → 20.7s（整体 24s）。截图改为 `E2E_SCREENSHOTS=1` 按需；删除 41 处 `waitForTimeout(300)`；`reducedMotion`；`workers` 上限 32。

**仍然遗留**

`1024/1280` 的 before 仍是重建件（`bands/`，已标注 reconstructed）。闭合方式见 `docs/11-ui-polish-handoff.md` 末节：接受重建基线，或提供 16:48 的旧 `web` 构建重截。
