# UI 细节打磨证据

对应报告：`docs/14-ui-detail-polish.md`。

## 内容

| 目录 | 数量 | 内容 |
| --- | --- | --- |
| `before/` | 69 | 改动前：9 路由 × 1440/768 × 深/浅（36 对）+ 12 状态对 + rich e2e 状态 |
| `after/` | 69 | 改动后：同一 36 对 + 12 状态对 + rich e2e 状态 |

9 条路由为 workspace / project / thread / inbox / settings / compare / login / invite / not-found。36 组路由与 12 组状态均同名成对。

## 状态对

| 状态 | before | after |
| --- | --- | --- |
| thread-empty 1440-dark / 768-light | 改动前源码副本 | 当前源码 |
| thread-error 1440-dark / 768-light | 改动前源码副本 | 当前源码 |
| workspace-loading 1440-dark | 改动前源码副本 | 当前源码 |
| workspace-error 1440-dark | 改动前源码副本 | 当前源码 |
| comment-dialog 1440-dark / 768-light | 改动前源码副本 | 当前源码 |
| confirm-dialog 1440-dark / 768-light | 改动前源码副本 | 当前源码 |
| new-project 1440-dark | 改动前源码副本 | 当前源码 |
| approvals-drawer 1440-dark | 改动前源码副本 | 当前源码 |

before 状态图来自一份保留原有未提交交互审计改动、但反转了本轮 UI 打磨改动的源码副本；使用同一 fixture server 和同一路由/状态脚本拍摄。这样可以在不触碰工作树未提交改动的前提下做真实的前后对照。

## 复验命令

```bash
cd /home/tw/Work/threadbase

# 36 路由矩阵 + 12 状态（36 + 12 张，写入 after/）
E2E_UI_EVIDENCE=1 pnpm --dir web exec playwright test tests/e2e/ui-evidence.spec.ts

# rich e2e 状态（41 张，写入 web/test-results/screenshots/）
E2E_SCREENSHOTS=1 pnpm --dir web exec playwright test
cp web/test-results/screenshots/*.png docs/screenshots/mifb/after/

# 10% 动效、命中区、触屏 chevron、按压缩放、hover、reduced-motion、横向溢出
E2E_UI_PROBE=1 pnpm --dir web exec playwright test tests/e2e/ui-probe.spec.ts
```

`ui-evidence.spec.ts` 与 `ui-probe.spec.ts` 都是 opt-in spec；默认 `pnpm --dir web exec playwright test` 只跑 6 个产品 e2e，不会重写证据目录。

## 10% 动效方法

DevTools Animations 面板的 10% playback 通过把过渡时长放大 10 倍、在 10% 时刻采样来复现（例如 300ms → 3s，300ms 采样；120ms → 1.2s，120ms 采样）。探针同时验证终态、命中区尺寸、相邻目标不重叠和 reduced-motion 下的 `transition-duration: 0s`。
