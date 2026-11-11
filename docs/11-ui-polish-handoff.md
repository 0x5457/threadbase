# UI polish 交接（已闭合）

完整上下文见 `docs/10-ui-polish-audit.md` §13–§14。本文档记录收尾状态与证据位置。

## 现状

代码、截图、探针、文档四条线都已闭合；`1024/1280` 的 before 基线仍是重建件，见文末「唯一遗留」。

复验（2026-11-11）：

```
pnpm --dir web typecheck   # 0 error
pnpm --dir web lint        # 0 error 0 warning
pnpm --dir web build       # ok
pnpm --dir web exec playwright test   # 1 passed, 24s
```

## 源码基线是怎么恢复的

上一轮工作树被并发会话覆盖，`git HEAD` 里是更旧的 Tailwind/shadcn 版本（依赖已不在 `node_modules`，无法构建）。审计修复只存在于截图里，源码丢失。

恢复来源：`/tmp/threadbase-before/web`（Mantine 基线快照，创建于审计开始前）。该快照**不含审计修复**，因此 58 条修复是在它之上重新落地的。该临时快照在收尾后已删除，修复本身已全部提交（`1f25b12`…`d10da54`）。

## 本轮改动（3 个 refactor + 5 个修复提交）

### 1. 设计系统

| 文件 | 内容 |
| --- | --- |
| `web/src/lib/design/tokens.ts` | 颜色/间距/字号/圆角/阴影/动效/断点/布局常量 |
| `web/src/lib/design/index.ts` | 汇总导出 |
| `web/src/main.tsx` | 令牌接入 Mantine theme；`respectReducedMotion: true`；组件默认值（Button/ActionIcon/Badge/Input/Menu/Modal/Drawer…） |
| `web/src/index.css` | 全局重置、滚动条、选中色、7 个动画关键帧、工具类（`.tb-mono` 等） |
| `web/src/components/ui/` | `ListRow`（+`RowText`）、`DataBadge`、`SectionHeader` |

### 2. 大文件拆分

| 文件 | 前 | 后 |
| --- | --- | --- |
| `components/thread/ThreadTimeline.tsx` | 1577 | **307** |
| `components/shell/SidebarTree.tsx` | 1486 | **408** |
| `routes/ThreadPage.tsx` | 1104 | **936** |

新增模块：

- `components/thread/timeline/`：`types.ts`、`items.ts`（事件 → 时间线投影，纯函数）、`toolIcons.ts`、`MessageRow`、`ToolCard`、`NoticeRow`、`PlanCard`、`SubagentRow`、`AgentMessageRow`、`ApprovalRow`
- `components/shell/sidebar/`：`status.ts`、`StatusDot`、`TreeRow`（含 `RowChevron`）、`treeNav.ts`（排序 + 方向键导航）、`rowStyles.ts`、`ThreadNode`、`SubagentNode`、`ProjectNode`、`ProjectRow`、`SidebarHeader`、`SidebarFooter`、`SidebarGroupLabel`
- `components/thread/SubagentView.tsx`（从 ThreadPage 抽出）

### 3. 内联字号清零

49 处 `fontSize: 11|12|13|14` 全部改为 `var(--mantine-font-size-xs|sm|md|lg)`。全仓不再有脱离主题档位的字号。

### 4. 审计条目修复（本轮新增/回归修正）

| 条目 | 改动 |
| --- | --- |
| TH1 | Composer 容器改为与时间线完全相同的 `px="md"` + `maxWidth: 768`，四档实测 `aligned=true`（此前 1280/1440 下 Composer 768 vs 时间线 736） |
| WS4 | 项目行恢复 `<sm` 的 threads/waiting 摘要（重写时丢失） |
| CP2 | Compare 的 Select 与卡片共用同一个两列 grid（重写时曾被限宽 480px） |
| NF2 | Invite 的 403/404/401 映射为可读文案，不再渲染 `ApiError: origin is not allowed` |
| SH3 | 无 thread 的页面不再渲染死的 Share 按钮（`onShare` 为空即不渲染） |
| G7 | 「Following」药丸的 `fontSize: 10` 改为 `size="compact-xs"` |

## 证据

`docs/screenshots/ui-polish/after/`：**72 张**（9 路由 × 4 宽度 × 2 主题）+ `probe.json`。

| 内容 | 路径 |
| --- | --- |
| after 全量 | `after/{workspace,project,thread,inbox,settings,compare,login,invite,notfound}-{768,1024,1280,1440}-{dark,light}.png` |
| 探针 | `after/probe.json`（64 条；login 为匿名上下文，不记探针） |
| before（1440/768 原始） | `before/`（36 张） |
| before（1024/1280 重建） | `bands/` + `bands/README.md`（明确标注 reconstructed） |

### 探针结论（`after/probe.json`）

- 横向溢出：**64/64 无**（`noHorizontalOverflow: true`）
- h1 文本左缘：全 5 个列表/表单页在四档一致（1440 均为 `x=512`，图标纹理起点 492）
- 列表行：`x=492 / w=736 / h=45`，`paddingLeft=12`（Workspace / Project / Inbox 相同）
- 卡片圆角：`8px`
- Compare：`grid-template-columns: 364px 364px`，两个 Select 落在同一列（1440 `x=492 / 864`）
- 正文基准字号：`13px`

### 截图环境

后端 `127.0.0.1:7791`（`/tmp/tb-shots`，`scripts` 同款夹具），前端用生产构建，身份头 `tailscale-user-login: taowei`；种子数据为 1 个项目 + 1 个线程 + 2 条消息 + 1 个待审批。截图脚本为临时文件，已删除。

## e2e 提速

| | 前 | 后 |
| --- | --- | --- |
| 测试本体 | 37.0s | **20.7s** |
| 整体 | ~40s | **24s** |

- 40 张 `fullPage` 截图改为按需：`E2E_SCREENSHOTS=1` 才执行
- 删除 41 处 `waitForTimeout(300)`（合计 12.3s）
- `reducedMotion: "reduce"` + `respectReducedMotion: true`，动画不再需要等待
- `workers` 上限 32（`E2E_WORKERS` 可覆盖），文件级仍串行，因为单个 spec 共享服务端夹具
- 顺带修掉一个被睡眠掩盖的真 bug：新建线程菜单内嵌 `Select` 的选项 portal 被判定为外部点击，导致表单被卸载

## 唯一遗留

`1024/1280` 的 **before** 基线是重建件（原始捕获为纯色空白，`colors=1`），`bands/README.md` 已标注 reconstructed。

这条不影响 after 证据与代码质量，但严格意义上审计不接受重建 before 替代原始 before。闭合方式二选一：

1. 明确接受重建基线 → 把成功标准 1 改为「1440/768 原始 before + 4 宽度 after；1024/1280 使用已标注的 reconstructed 基线」；
2. 提供 2026-11-10 16:48 前后的 `web` 工作树备份/构建，用它重截 1024/1280 的 before 覆盖 `bands/`。

## 复验命令

```bash
cd /home/tw/Work/threadbase
pnpm --dir web typecheck
pnpm --dir web lint
pnpm --dir web build
pnpm --dir web exec playwright test                  # 24s
E2E_SCREENSHOTS=1 pnpm --dir web exec playwright test  # 额外写 40 张截图
```
