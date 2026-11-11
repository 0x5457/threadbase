# UI 细节打磨审计（make-interfaces-feel-better · full）

基准：`docs/05-ui-spec.md`（Linear / Apple Notes 方向、8px 圆角、11–14px 信息层级）、`docs/10-ui-polish-audit.md`、`docs/13-interaction-audit.md`。

模式：**full**。范围：`web/src` 全部路由、外壳、覆盖层、subagent 变体；1440/768 桌面宽度、深/浅主题、thread 390 触屏命中区。
框架与样式约定：React 19 + Mantine 9 + `lib/design/tokens.ts` + `index.css`；不引入第二套样式系统。

边界：不做 Rust/API/产品流程重设计；不重开 `docs/10`–`13` 的历史争议与 P2 backlog；不把本轮发现扩成信息架构改造。

---

## 1. 检查范围与证据

### 1.1 路由与状态

| 路由 | 入口/覆盖层 | 已检查状态 |
| --- | --- | --- |
| `/` Workspace | 项目行、Add project、侧栏 | 加载、空、错误、深/浅、1440/768 |
| `/p/:id` Project | 线程行、Compare、Archived、New thread 菜单 | 加载、空、错误、新建线程 Modal、深/浅、1440/768 |
| `/p/:id/t/:tid` Thread | 时间线、composer、标题/面包屑、右栏、审批/提问/评论 | 空、加载、错误、断线、resync、审批、只读、深/浅、1440/768/390 |
| `/inbox` Inbox | Unread/All、空态 | 加载、空、深/浅、1440/768 |
| `/settings` Settings | 表单、主题、standing rules | 加载、错误、未保存、深/浅、1440/768 |
| `/p/:id/compare` Compare | 双 Select、指标卡 | 空、单/双线程、深/浅、1440/768 |
| `/login` Login | 匿名上下文、主题切换 | 默认、错误、深/浅、1440/768 |
| `/invite/:token` Invite | 接受失败态 | 403/404 文案、深/浅、1440/768 |
| `*` NotFound | 返回项目 | 深/浅、1440/768 |
| 外壳/浮层 | TitleBar、Sidebar、StatusBar、CommandPalette、RightDrawer、Share/NewProject/Confirm Modal | hover/focus/active/empty/disabled、深/浅、1440/768 |
| Subagent | `SubagentView`、`SubagentComparePage`、picker | 空、深/浅、1440/768 |

### 1.2 截图与命令

- 证据目录：`docs/screenshots/mifb/before/` 69 张，`after/` 69 张。9 路由 × 1440/768 × 深/浅 = **36 对**，12 组空/加载/错误/对话框/右栏状态 = **12 对**，两侧同名；其余是 rich e2e 状态（changes、composer、share、subagents、reconnecting、diverged、390 触屏）。
- 状态对：`thread-empty-*`、`thread-error-*`、`workspace-loading-1440-dark`、`workspace-error-1440-dark`、`comment-dialog-*`、`confirm-dialog-*`、`new-project-1440-dark`、`approvals-drawer-1440-dark` 均为同名 before/after。before 状态图来自保留既有未提交交互审计改动、但反转本轮 UI 打磨改动的源码副本；来源与命令见 `docs/screenshots/mifb/README.md`。
- 复验命令：`E2E_UI_EVIDENCE=1 pnpm --dir web exec playwright test tests/e2e/ui-evidence.spec.ts`（36 路由矩阵 + 12 状态）；`E2E_SCREENSHOTS=1 pnpm --dir web exec playwright test`（41 张 rich 状态）；`E2E_UI_PROBE=1 pnpm --dir web exec playwright test tests/e2e/ui-probe.spec.ts`（10% 动效、命中区、溢出探针）。三个 spec 都是 opt-in，默认套件不运行。
- 基线门：`pnpm --dir web typecheck`（0 error）、`pnpm --dir web lint`（0 error / 0 warning）、`pnpm --dir web build`（成功）、`E2E_SCREENSHOTS=1 pnpm --dir web exec playwright test`（6 passed / 0 failed）。
- 机械检查：`rg "transition:\s*all|transition-all|will-change:\s*all" web/src` 无结果；`rg "fz=\{?[0-9]|fontSize:\s*[0-9]" web/src` 无结果；`rg "tb-hit-32" web/src` 无结果。

### 1.3 五类覆盖

| 类别 | 实际检查的证据 | 结果 |
| --- | --- | --- |
| Typography | 全部 `Text`/`Title` 用法、`fontSizes` token、登录/设置/时间线/Compare 截图 | T1–T3 |
| Surfaces | 嵌套 Paper/Input/Button、头像、diff 行、树行、触屏 CSS、截图 | S1–S3 |
| Animations | 主题切换、`<details>`、按钮按压、`index.css` keyframes、reduced-motion | A1–A3 |
| Icons | lucide 尺寸/线宽、语义、状态、`currentColor`、图标库 | I1–I2 |
| Performance | transition 属性、`will-change`、死代码工具类、构建 | P1；`transition: all`/`will-change: all` **Clear** |

---

## 2. Findings

### Typography

#### T1 · 内联数字字号绕过 11–14px 字阶

| Severity | Location | Before | After | Why |
| --- | --- | --- | --- | --- |
| MEDIUM | `src/components/thread/timeline/MessageRow.tsx:52,57,63`；`PlanCard.tsx:42,58`；`SubagentRow.tsx:80`；`ElicitationCard.tsx:131`；`ChangesPanel.tsx:410,479,622`；`FilesPanel.tsx:154,286`；`Composer.tsx:296,388,582,794,806,810`；`LoginPage.tsx:43` | `fz={12}` / `fz={13}` / `fz={11}` / `fz={22}` 等 18 处裸数字 | 分别改为 `size="sm"` / `size="md"` / `size="xs"`；登录页移除 `fz={22}`，回到主题 h1 | 字号脱离 `fontSizes` 档位后，同一层级在不同页面漂移；这也是仓库“零内联字号”约定的回归 |

#### T2 · 标题与短正文没有换行优化

| Severity | Location | Before | After | Why |
| --- | --- | --- | --- | --- |
| LOW | `src/index.css`（全局）；`Title`/`Text` 渲染的 h1–h3 与正文 | 浏览器默认换行 | `h1,h2,h3 { text-wrap: balance }`；`.mantine-Text-root { text-wrap: pretty }` | 长 project/thread 标题与说明文字会出现孤行；balance 只作用于短标题，pretty 适合短中正文 |

#### T3 · 动态计数没有统一等宽数字

| Severity | Location | Before | After | Why |
| --- | --- | --- | --- | --- |
| LOW | `src/components/shell/TitleBar.tsx:213-243`；`src/routes/ThreadPage.tsx:815,830,843`；`src/components/ui/DataBadge.tsx`；`src/routes/InboxPage.tsx:142-153` | 审批 Indicator、waiting/queued Badge、inbox 未读数使用比例数字 | `.mantine-Badge-root` 与 TitleBar `Indicator` 加 `font-variant-numeric: tabular-nums`；Inbox 未读数用同一属性；StatusBar/`tb-mono` 保持 | 计数更新时宽度跳动，密集工具栏尤其明显 |

### Surfaces

#### S1 · 4px 内边距的嵌套面仍沿用 8px 圆角

| Severity | Location | Before | After | Why |
| --- | --- | --- | --- | --- |
| MEDIUM | `src/components/thread/SelectionComment.tsx:70-76,94-116`；`ToolCard.tsx:181-196,201,220`；`Composer.tsx:352-382`；`ElicitationCard.tsx:129-140`；`ApprovalRow.tsx:100-116` | 外层 `Paper` 8px + `p="xs"`(4px)，内层 `Textarea`/`pre`/queue row 仍为 8px | 内层改 `radius="sm"` / `var(--mantine-radius-sm)`（4px），满足 `8 = 4 + 4` | 内层圆角与外侧边几乎相切时，等半径会让角部看起来“被夹住”；这是截图里选区评论框和工具卡展开区最明显的表面问题 |

#### S2 · 头像图片没有中性 1px 轮廓

| Severity | Location | Before | After | Why |
| --- | --- | --- | --- | --- |
| LOW | `src/components/app/AvatarStack.tsx:48-54`；`MessageRow.tsx:39-46`；`InboxPage.tsx:56-62`；`SettingsPage.tsx:367-372`；`SidebarFooter.tsx:95-101` | 有 `avatarUrl` 时直接渲染图片，无 outline | `.mantine-Avatar-image { outline:1px solid oklch(0 0 0 / .1); outline-offset:-1px }`；dark 下 `oklch(1 0 0 / .1)` | 头像图片落在不同底色上时边缘不稳定；技能要求纯黑/纯白低透明度，不能用带色中性色 |

#### S3 · 命中区低于 40/44px，关键操作只有 18–30px

| Severity | Location | Before | After | Why |
| --- | --- | --- | --- | --- |
| HIGH | 触屏 CSS `index.css:303-330`；审批 `ApprovalRow.tsx:190-230`；队列 `Composer.tsx:380-404`；评论删除 `CommentsPanel.tsx:289-300`；standing rule `ApprovalRulesSection.tsx:100-108`；消息工具条 `MessageRow.tsx:71-99`；`ToolCard.tsx:51-57`；`ElicitationCard.tsx:105-112`；TitleBar `TitleBar.tsx:88-256`；Thread 头 `ThreadPage.tsx:755-780`；右栏头 `RightDrawer.tsx:154-174`；侧栏 `SidebarFooter.tsx:143-172`；`TreeRow.tsx:67-95` | 触屏最小 40px；桌面 `ActionIcon` 18/22/24/28、`compact-xs` 22、`size="xs"` 30；树 chevron 16/28/36；多处相邻按钮仅 4px 间距 | 触屏统一 44px；关键文字操作提升到 `size="sm"` 并加 `min-height:40`；独立图标按钮用 `::after` 扩到 40（触屏 44）；相邻工具条补足 gap，使 40 命中区不重叠；消息工具条 24 视觉 / 40 命中；树行/chevron 桌面 40、触屏 44；列表行 40；全局 Button/Input/Checkbox 40；StatusBar 40 | 审批、撤回、删除、评论这些高频/高风险动作的可点区域小于指尖/鼠标稳定命中范围；技能要求桌面 ≥40、触屏 ≥44，且命中区不得重叠 |

### Animations

#### A1 · 主题切换图标瞬时替换

| Severity | Location | Before | After | Why |
| --- | --- | --- | --- | --- |
| MEDIUM | `src/components/shell/ThemeToggle.tsx:16-19` | `{theme === "dark" ? <Sun/> : <Moon/>}` 条件渲染，无 enter/exit | 两个图标同时留在 DOM，绝对定位交叉淡入；`opacity 0→1`、`scale .25→1`、`blur(4px)→0`，300ms `cubic-bezier(.2,0,0,1)` | 图标状态切换是典型的 contextual icon animation；直接替换让状态变化显得生硬，且暗/亮切换是低频操作，适合完整 enter/exit |

#### A2 · `<details>` 展开态没有视觉指示

| Severity | Location | Before | After | Why |
| --- | --- | --- | --- | --- |
| MEDIUM | `ThreadTimeline.tsx:210-214`；`ToolCard.tsx:138-144`；`SubagentRow.tsx:46-52`；`AgentMessageRow.tsx:49-55`；`ToolCard.tsx:225-235`；`ApprovalRow.tsx:140-150`；`RightDrawer.tsx:292-302` | ChevronRight 永远朝右；`Details` / `Full parameters` summary 没有 chevron | 给 chevron 加 `data-disclosure-chevron`，CSS 在 `details[open] > summary` 时旋转 90°（120ms transition）；`Details`/`Full parameters` 补一个同款 chevron | 展开/收起是状态变化，图标不反映状态会让用户以为点击无效；旋转是静态线索，不依赖动画 |

#### A3 · 主操作按钮没有按压缩放反馈

| Severity | Location | Before | After | Why |
| --- | --- | --- | --- | --- |
| LOW | `src/index.css`（全局）；所有 `data-variant="filled"` 的 Mantine Button | `:active` 只有颜色变化 | `.mantine-Button-root[data-variant="filled"]:active:not(:disabled) { scale:.96 }`，150ms ease-out；reduced-motion 下取消缩放 | 主操作需要触觉反馈；0.96 是技能规定的唯一值。只作用于 filled 主按钮，避免给列表行/高频小控件增加动效 |

### Icons

#### I1 · lucide 图标统一 2px 线宽，与 400/500 文字不匹配

| Severity | Location | Before | After | Why |
| --- | --- | --- | --- | --- |
| MEDIUM | 全站 lucide 图标（约 100 处 `size={12..16}`） | 默认 `stroke-width="2"` | `svg.lucide { stroke-width:1.5 }`；badge 内保持 2px | 12–16px 图标配 2px 线在常规字重旁显得过重、发黑；技能要求 1.5px 配 regular、2px 配 semibold。全局统一避免逐处漏改 |

#### I2 · 提问卡的“评论”动作使用了 HelpCircle

| Severity | Location | Before | After | Why |
| --- | --- | --- | --- | --- |
| MEDIUM | `src/components/thread/ElicitationCard.tsx:105-112` | `HelpCircle` 图标，`aria-label="Comment on question"` | `MessageSquare` 图标 | 同一张卡头部已经用 HelpCircle 表示“需要输入”；评论动作再用同一图标会被读成“帮助”，语义冲突 |

### Performance

#### P1 · 未使用的动效工具类与 bouncy keyframe

| Severity | Location | Before | After | Why |
| --- | --- | --- | --- | --- |
| LOW | `src/index.css:190-254` | `fade-in`、`fade-in-up`、`fade-in-down`、`scale-in`、`slide-in-*`、`.tb-animate-*`、`.tb-transition`、`.tb-truncate`、`.tb-no-select`、`.tb-sr-only` 均无引用；`scale-in` 使用 `cubic-bezier(.34,1.56,.64,1)` 过冲曲线 | 删除未引用 keyframes/工具类；保留 `sidebar-pulse`、`spin`、`.tb-mono` | 死代码增加 CSS 体积，且 `scale-in` 的过冲曲线与技能“ease-out、克制”相冲突，容易被后续误用 |

---

## 3. Considered but Rejected

| Location | Candidate | Rejected because |
| --- | --- | --- |
| diff / code 行 | 逐行扩到 40px | 代码行是密集数据网格，逐行 40px 会破坏可读性；整行宽度足够，保留密集行高。普通列表行、树行和表单控件均为 40+ |
| 所有卡片 | 把 8px 外层圆角改成 16px 以满足严格同心 | 会改变已收敛的 8px 视觉方向；只修复 4px 内边距的紧贴嵌套面，深内边距表面按独立表面处理 |
| 全部按钮 | 给所有 Button/ActionIcon 加 `scale(.96)` | 列表行、树行、文件行属于高频交互，技能要求不对高频交互加自定义动画；只给 filled 主操作 |
| 页面首次进入 | 给每个路由加 stagger 入场 | 常规导航不是“低频 staged entrance”，stagger 会拖慢高频切换；技能明确要求克制 |
| 所有动画元素 | 预加 `will-change` | 没有观察到首帧抖动；`will-change` 只应在确有性能问题时使用，否则增加合成层内存 |

---

## 4. 逐项修复状态

> 本表在收口时更新为最终状态。任务 2–6 每完成一批修复，就把对应行从 `Open` 改为 `Fixed` 并补代码位置；任务 7 复查。

| ID | Severity | Status | Fix locations |
| --- | --- | --- | --- |
| T1 | MEDIUM | Fixed | `MessageRow`、`PlanCard`、`SubagentRow`、`ElicitationCard`、`ChangesPanel`、`FilesPanel`、`Composer`、`LoginPage`；18 处裸数字改为 `size`/`fz` 主题档位，登录 h1 回到主题 h1 |
| T2 | LOW | Fixed | `index.css`：h1–h4 `text-wrap:balance`，`.mantine-Text-root` `text-wrap:pretty` |
| T3 | LOW | Fixed | `index.css`：`.mantine-Badge-root { font-variant-numeric: tabular-nums }`；`TitleBar` approvals `Indicator` 与 `Inbox` 未读数同样加 tabular-nums |
| S1 | MEDIUM | Fixed | `SelectionComment`、`ToolCard`、`Composer` queue row、`ElicitationCard`、`ApprovalRow`：4px 内边距的内层改为 4px 圆角 |
| S2 | LOW | Fixed | `index.css`：`.mantine-Avatar-image` 纯黑/纯白 10% 轮廓，dark 覆盖 |
| S3 | HIGH | Fixed | `index.css` 全局 Button/Input/Checkbox/CloseButton 40px（CloseButton 用 40px `::after`）、触屏 44px；`tb-action-40`/`tb-hit-40`（`tb-hit-32` 已移除）；列表行/树行 40px；树 chevron 桌面 40 / 触屏 44；split 按钮 40×40；StatusBar 40；TitleBar、Thread 头、RightDrawer、SidebarFooter、MessageRow、ToolCard、Elicitation、ApprovalRow、CommentsPanel、ChangesPanel、Composer、ApprovalRules、ShareDialog、ConfirmDialog、Project/Inbox/Login/Workspace/Invite/NotFound/WorkspaceLayout |
| A1 | MEDIUM | Fixed | `ThemeToggle.tsx` + `index.css`：双图标常驻，opacity/scale/blur 300ms `cubic-bezier(.2,0,0,1)` |
| A2 | MEDIUM | Fixed | `ThreadTimeline`、`ToolCard`、`SubagentRow`、`AgentMessageRow`、`ApprovalRow`、`RightDrawer`：`data-disclosure-chevron` + `details[open]` 旋转；补 Details/Full parameters chevron |
| A3 | LOW | Fixed | `index.css`：filled 按钮 `:active { scale:.96 }`，150ms ease-out；reduced-motion 取消 |
| I1 | MEDIUM | Fixed | `index.css`：`svg.lucide { stroke-width:1.5 }`；badge 内保持 2px |
| I2 | MEDIUM | Fixed | `ElicitationCard.tsx`：评论动作 `HelpCircle` → `MessageSquare` |
| P1 | LOW | Fixed | `index.css`：删除未引用 keyframes/工具类（`fade-in*`、`scale-in`、`slide-in-*`、`tb-animate-*`、`tb-transition`、`tb-truncate`、`tb-no-select`、`tb-sr-only`、`spin`） |

---

## 5. Verification

### 5.1 基线

```
pnpm --dir web typecheck   # 0 error
pnpm --dir web lint        # 0 error / 0 warning
pnpm --dir web build       # ok
E2E_SCREENSHOTS=1 pnpm --dir web exec playwright test  # 6 passed, 1 skipped
```

### 5.2 Before 证据

`docs/screenshots/mifb/before/`（69 张）与 `after/`（69 张）：36 组 9 路由 × 1440/768 × 深/浅 + 12 组状态全部同名成对。before 状态图来自反转本轮改动、保留既有交互审计改动的源码副本（见 README）。前后对照：1440/768 均无横向溢出；thread 首屏从 before 的 “Jump to latest” 变为直接落在最新消息。

### 5.3 已知回归（本轮必须修复）

| 现象 | 证据 | 状态 |
| --- | --- | --- |
| 打开有历史的 thread 时，时间线停在顶部并显示 “Jump to latest”，而不是跟随最新消息；`transcript: follows new events and offers jump-to-latest` 曾一次失败 | before `thread-1440-light.png` 首屏可见 Jump to latest；after 首屏直接落在最新消息 | **Fixed**：`ThreadPage.tsx` 用 `didInitialScroll`/`lastThreadId` 在首次数据到达时滚到底部；`atBottom` 绑定 threadId，避免 setState-in-effect |

### 5.4 收口验证（2026-11-11）

**质量门**

```
pnpm --dir web typecheck   # 0 error
pnpm --dir web lint        # 0 error / 0 warning
pnpm --dir web build       # ok
E2E_SCREENSHOTS=1 pnpm --dir web exec playwright test  # 6 passed, 0 failed
```

**证据复验**

```
E2E_UI_EVIDENCE=1 pnpm --dir web exec playwright test tests/e2e/ui-evidence.spec.ts
# 36 路由矩阵 + 12 状态；与 E2E_SCREENSHOTS 的 41 张 rich 图取并集后为 after/ 的 69 张。
E2E_UI_PROBE=1 pnpm --dir web exec playwright test tests/e2e/ui-probe.spec.ts
# 10% 动效、命中区、触屏 chevron、按压缩放、reduced-motion、横向溢出。
```

**10% 动效与状态探针**（永久 opt-in spec，非临时脚本）

10% 重放通过把过渡时长放大 10 倍并在 10% 时刻采样，等价于 DevTools Animations 面板的 10% playback；这样不依赖 DevTools UI，且可重复执行。

| 检查 | 结果 |
| --- | --- |
| 主题图标 10% 重放（300ms → 3s，300ms 采样） | 13 个动画运行中；出场 `opacity .883 / scale .912 / blur .468px`，入场 `opacity .117 / scale .338 / blur 3.532px` |
| Disclosure 10% 重放（120ms → 1.2s） | 中点 `matrix(0.9993, 0.0382, -0.0382, 0.9993, 0, 0)`，终态 `matrix(0, 1, -1, 0, 0, 0)`（90°） |
| 主按钮按压缩放 | filled 按钮按下 `scale: .96`；`Add project` `min-height: 40px` |
| 命中区扩展 | 审批图标 28 视觉 / 40 `::after`；抽屉图标 22 / 40；消息评论 24 / 40；footer 主题按钮 28 / 40；桌面树 chevron 40×40、触屏 44×44；split 按钮 40×40 且与主 New thread 不重叠；New project 返回箭头 40；列表行 40+；全局 Button/Input/Checkbox 40；settings 主题按钮 40；6 组相邻目标 `overlap=false`；`tb-hit-32` 已移除 |
| 横向溢出 | 1440 = 0px；768 = 0px |
| reduced-motion | e2e 全程以 `reducedMotion: reduce` 跑通；探针读到主题图标 `transition-duration: 0s`；静态颜色/图标/文案线索保留 |

**状态走查**

| 状态 | 证据 | 结果 |
| --- | --- | --- |
| hover | `ui-probe.spec.ts`：项目行背景 `rgba(0,0,0,0)` → `rgba(59,59,59,.84)`；行内过渡只改 `background-color` | 通过；静态颜色变化，无入场动画 |
| focus | `aa-interactions.spec.ts › keyboard: roving tree, visible focus, shortcuts stay out of the composer`（通过）；全局 `:focus-visible` 2px outline | 键盘焦点可见，树行 roving focus 正常 |
| active | `ui-probe.spec.ts` filled 按钮按下 `scale: .96` | 通过 |
| loading | `workspace-loading-1440-dark.png`（before/after） | 骨架屏可见 |
| empty | `thread-empty-1440-dark.png`、`thread-empty-768-light.png`（before/after 成对） | 空态文案与图标正常 |
| error | `thread-error-*`、`workspace-error-1440-dark.png`（均为 before/after） | 错误态与重试入口正常 |
| disabled | `ui-probe.spec.ts` 空 composer 的 Send 按钮 `disabled` | 通过 |

**逐条复查**：T1–T3、S1–S3、A1–A3、I1–I2、P1 均为 Fixed；`rg "transition:\s*all|transition-all|will-change:\s*all" web/src` 无结果；`rg "fz=\{?[0-9]|fontSize:\s*[0-9]" web/src` 无结果。

### 5.5 Verdict

**Approve** — 无 actionable interface-polish findings。5 条建议因密度、8px 视觉方向或高频交互被列入 Considered but Rejected，不作为待办。未验证项：无；12 组状态与 36 组路由均为 before/after 成对。
