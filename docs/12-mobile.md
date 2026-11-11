# 移动端体验修复（2026-11-11）

目标：手机浏览器（iPhone Safari / Android Chrome）上不再"自己放大"，并且键盘、触控、抽屉、输入框都按手机尺寸工作。桌面端（fine pointer）保持原有密度。

## 根因

| 现象 | 根因 |
| --- | --- |
| 点输入框后整页放大，且不再缩回 | 所有表单字号来自桌面 token（11–14px）。iOS Safari 在聚焦 `<16px` 的 input/textarea 时会自动缩放页面 |
| 键盘弹出后输入框被挡 | 外壳用 `height: 100%` / `100dvh`；iOS 键盘只改变 visual viewport，不改变 layout viewport |
| 顶栏、侧栏图标难按 | `ActionIcon` 22–28px，按钮 30px，低于拇指可点范围 |
| 右抽屉超出屏幕 | `phone` 属性从未传入，Drawer 固定 420px（手机宽 390px） |
| 状态栏互相重叠 | 手机上同时显示项目名、分支、diff、0 running、0 waiting |
| "Sent" 提示盖住 Send 按钮 | Notifications 固定在 bottom-right |
| 登录页 Tailscale 命令被裁切 | `Code block` 为 `white-space: pre`，把 Alert 内容撑宽后整体裁切 |

## 改动

| 文件 | 内容 |
| --- | --- |
| `web/index.html` | viewport 增加 `interactive-widget=resizes-content`（Android 键盘缩布局） |
| `web/src/lib/viewport.ts` | 新增：把 `visualViewport.height` 写入 `--tb-app-height`；键盘弹出时外壳、时间线、Composer 一起缩到键盘上方 |
| `web/src/index.css` | `100dvh` + `--tb-app-height`；`text-size-adjust`；`overscroll-behavior`；`(pointer: coarse)` 下：输入框 `16px !important`、控件 ≥40px、手机字号档位、`touch-action: manipulation`、状态栏/Toast 规则 |
| `AppShell` | 右侧抽屉传入 `phone`（全宽）；侧栏在窄屏首帧即关闭 |
| `TitleBar` | 手机上搜索改为图标按钮；无待审批时隐藏审批入口，给标题让位 |
| `StatusBar` | 手机线程页隐藏重复的项目名与空计数；栏高与内容一致（32px） |
| `ThreadPage` | 手机上的第二行改为"项目"面包屑（可返回），线程标题只在标题栏出现 |
| `Composer` | 手机上卡片内边距交给内部 Box；草稿最小 2 行；高度上限跟随 `--tb-app-height`；model/effort 选择器在窄屏定宽并允许换行 |
| `AppNotifications` | 手机 Toast 移到标题栏下方；删除各处写死的 `position: "bottom-right"`，统一由容器决定 |
| `LoginPage` | 命令与 URL 允许换行，320px 下不再裁切 |

## 验证

```
pnpm --dir web typecheck   # 0 error
pnpm --dir web lint        # 0 error 0 warning
pnpm --dir web test:e2e    # 1 passed（含新增手机断言）
```

`web/tests/e2e/core.spec.ts` 新增手机上下文断言：

- 390px 下 `scrollWidth === innerWidth`（无横向溢出）
- `textarea[placeholder="Message"]` 计算字号为 `16px`
- 视口缩到 500px 高时 `--tb-app-height === "500px"`
- 右抽屉宽度为 390px（全宽）

手工探针覆盖 320 / 390 / 430px：无横向溢出；可点目标 ≥36px（状态栏 diff 按钮为 28px，受 32px 栏高限制）；键盘模拟（视口 390×360）下 Composer 底部位于视口内。

## 部署注意

线上 `:8443` 指向 `~/.local/bin/threadbase` 的 **release** 二进制，`web/dist` 在编译期内嵌；只跑 `pnpm build` 不会更新线上 UI。需要：

```
scripts/deploy.sh
```

该脚本会拒绝在脏工作区部署。当前工作区有未提交的后端改动，因此本轮**未部署**。
