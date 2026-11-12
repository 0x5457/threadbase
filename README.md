# Threadbase

暂定代号。一个面向小团队的 coding agent 协作工作台：以本地项目目录为一级维度，把 agent 对话、代码变更和审批放进可共享的 thread，队友可以实时观看、发言、接管和评审。

产品形态参考 [Delta](https://delta.dev/)，但不自建 agent runtime：底层复用 Claude Code、Codex、opencode、pi 等已有 agent，通过 ACP 接入。

## 当前状态

v1 已实现并通过验证。包含 Rust 单二进制服务、React 工作台、真实 ACP 会话、SQLite 事件日志、CRDT 草稿、审批/提问、Changes 审阅、评论、presence/跟随、分享与角色、CLI 与发布工作流。

质量门由 `scripts/verify.sh` 统一执行；界面审计记录保留在 `web/audit/`。

## 快速开始

前置：Rust 1.88+、Node 24+、pnpm 10、git。运行真实 agent 还需要对应 CLI 已安装并登录。

```bash
# 构建前端资源（单二进制会内嵌它）
pnpm --dir web install --frozen-lockfile
pnpm --dir web build

# 启动本地服务，默认 http://127.0.0.1:7777
MBX_BLAZE=false cargo run -p threadbase
```

服务启动后运行 `threadbase doctor` 检查 Node、git、数据库和 Codex/Claude/opencode/pi adapter。

开发前端时另开一个终端：

```bash
pnpm --dir web dev   # http://127.0.0.1:5173，/api 与 /ws 代理到 7777
```

真实 Codex 需要安装 `codex-acp` 并登录 Codex；服务会自动把本机 `codex` 路径写入 `CODEX_PATH`。所有参与者共用这台机器上的 agent 登录，成本与审计记在该账号。没有 adapter 时 `doctor` 会明确报告，UI 会把 thread 标记为错误而不是卡在运行中。

数据目录默认是平台数据目录下的 `threadbase/`（Linux 为 `~/.local/share/threadbase/`），可用 `THREADBASE_DATA_DIR` 覆盖。服务固定绑定回环地址，`THREADBASE_BIND` 只接受回环地址。

## 部署模型：本机服务，GitHub 登录，HTTPS 隧道对外

服务跑在开发机上，只监听回环地址；对外由任意 HTTPS 隧道终结 TLS 并转发到 `127.0.0.1:7777`。Tailscale Funnel 是最省事的一种：

```bash
tailscale funnel --bg --https=443 http://127.0.0.1:7777
```

其他设备（不需要装 Tailscale，也不需要加入 tailnet）访问 `https://<machine>.<tailnet>.ts.net`，用 GitHub 账号登录。

### 登录：GitHub OAuth + 服务端会话

- 应用不保存密码。浏览器通过 GitHub OAuth（授权码 + PKCE）登录，服务端在 `auth_sessions` 里存会话，只把哈希入库；浏览器拿到的 Cookie 是 `HttpOnly; SameSite=Lax`，生产环境再加 `Secure`。
- **完全不信任任何身份请求头。** 之前 `Tailscale-User-Login` 那套已经删除，公网隧道上伪造请求头无法提权。
- 需要在 GitHub 注册一个 **OAuth App**（不是 GitHub App）：`Settings → Developer settings → OAuth Apps → New OAuth App`，回调地址填 `https://<公开域名>/api/auth/github/callback`，权限不勾任何 scope。

```bash
export THREADBASE_PUBLIC_ORIGIN=https://machine.tailnet.ts.net
export THREADBASE_GITHUB_CLIENT_ID=...
export THREADBASE_GITHUB_CLIENT_SECRET=...
export THREADBASE_GITHUB_ALLOW=octocat,ada      # 谁可以登录
export THREADBASE_ADMINS=octocat                # 谁永远是管理员
cargo run -p threadbase
```

- 登录策略：管理员名单 > 允许名单 > 已存在用户 > 实例尚未有管理员时自动放行第一个登录者（引导）。因此公开隧道不会变成「任何 GitHub 用户都能进」。
- 首次登录的身份自动成为管理员；`THREADBASE_ADMINS` 可指定固定管理员。
- 会话默认 30 天，边用边续期（`THREADBASE_SESSION_DAYS` 可改）；退出登录会撤销服务端会话。
- 想撤销某人访问：从 `THREADBASE_GITHUB_ALLOW` 移除并在用户页取消管理员，或直接删 `auth_sessions` 里的会话；想重置本机开发环境，直接删 `threadbase.db`。

### 一键部署（systemd）

在 Linux 主机上一条命令把**当前这个二进制**配置成 systemd user 服务：

```bash
threadbase install --funnel --admin octocat
```

它会：

- 写 `~/.config/systemd/user/threadbase.service`，`ExecStart` 就是当前进程的可执行文件路径（不拷贝、不构建、不下载）
- `Environment` 里带上绑定地址、数据目录、`THREADBASE_PUBLIC_ORIGIN`（从 `tailscale status` 推导）与 `THREADBASE_ADMINS`，以及去重后的 `PATH`（agent CLI 常在 mise/nvm 路径下）
- 带上 `EnvironmentFile=-%h/.config/threadbase/env`：**GitHub 的 client secret 放这个文件，不写进 unit**（unit 文件是可读的）
- `loginctl enable-linger`，让服务在登出后仍在跑
- `systemctl --user enable --now`
- `tailscale serve|funnel --bg --https=<port> http://127.0.0.1:7777`（`--funnel` 才公开；GitHub OAuth 回调需要公开 HTTPS）
- 轮询 `/api/health` 确认真的起来了

所以要升级，替换这个二进制再跑一次 `threadbase install`（或 `threadbase upgrade` 后 `systemctl --user restart threadbase`）即可。

常用选项：

```bash
threadbase install --print                  # 只打印 unit 与命令，不改动系统
threadbase install --port 7788 --https-port 9443 --unit threadbase-dev
threadbase install --no-serve --no-start     # 只写 unit
threadbase install --funnel                 # 用 tailscale funnel 公开到互联网
threadbase install --uninstall              # 停用并删除 unit、关掉 serve/funnel（保留数据与二进制）
```

unit 名默认 `threadbase`；`--unit` 可在同一台机器上跑多个实例。日志：`journalctl --user -u threadbase -f`。

### 网页终端（默认关闭）

`THREADBASE_TERMINAL=1` 才挂载终端路由，且**只有管理员能驱动**：其他 `member` 账号打开同一个终端只能看（tmux 只读客户端，服务端不接收其按键），`read_only` 账号连入口都没有。关掉这个功能时相关路由返回 404，不暴露存在性。

```bash
THREADBASE_TERMINAL=1 cargo run -p threadbase
# 浏览器里按 Ctrl+` 或点左栏底部的终端图标
```

- **形态**：底部抽屉（`Ctrl+`` 开关，拖上边缘改高度，`⤢` 全屏）。每个 thread 一个 shell，cwd 是该 thread 的 worktree；未选中 thread 时开的是 `$HOME` 里的全局 shell（`h-<user>`）。
- **持久化在 tmux 内存里，不落盘**：会话名 `t-<thread>` / `h-<user>`，挂在私有 socket `threadbase` 上（`THREADBASE_TERMINAL_SOCKET` 可换）。关抽屉、刷新、重启服务都只是 detach，shell 继续跑；`history-limit=10000` 的 scrollback 也只存在 tmux server 内存里。
- **只读由 tmux 执行**：driver 用 `tmux attach`，其他人 `tmux attach -r`；`window-size manual` + `resize-window` 让窗口尺寸跟 driver 走，围观者小屏是裁切而不是让 driver 的终端换行。
- **鉴权**：WebSocket 不能带自定义头，所以升级用一次性 ticket（`POST /api/terminal/sessions`，受 CSRF 中间件保护，30 秒过期、用户与会话绑定）。升级时还要求会话 Cookie 与当前用户行仍然有效。
- **不记日志**：终端内容不写入磁盘、也不进事件日志；需要复盘时只能靠 tmux 里还活着的 scrollback。想彻底停掉就 `tmux -L threadbase kill-server`。
- **退出即提示**：shell 退出时客户端收到 EOF，抽屉显示 `The shell exited (code)` 和 `New session`，不会留下一个死掉的 pane。
- **在场感**：开着终端的人会在其他人的 composer 工具栏里显示一个终端图标（`Panel::Terminal` 走现有 presence 通道）；关掉抽屉但 shell 还活着时，状态栏留一个 `terminal` 小按钮。两者点击都能回到终端。
- **前端**：xterm.js 懒加载；按键条补上浏览器吞掉的键（Ctrl+C/D/L/Z/R、Esc、Tab、方向键）；`Ctrl+Shift+C/V` 复制粘贴，多行粘贴先确认。**手机端只读**：抽屉是全屏，ticket 本身是 watcher-only，服务端不会给这台设备键盘（软键盘压着一个 pty 是另一个产品）。

### 本机开发

`pnpm --dir web dev` 的请求来自回环但没有会话 Cookie，需要显式打开开发身份：

```bash
THREADBASE_DEV_IDENTITY=dev cargo run -p threadbase
```

开发身份只在「回环 peer + 无 `X-Forwarded-*` 头」时生效，因此经隧道进来的请求永远不会命中它。不要在生产实例上设置该变量。

### CSRF 与 Cookie

写操作要求 `Origin` 与 `Host` 一致（或等于 `THREADBASE_PUBLIC_ORIGIN`）并带 `X-Requested-With: threadbase`。隧道会保留浏览器的 Host，因此通常无需额外配置；如果前面还有别的反代改写了 Host，就设置 `THREADBASE_PUBLIC_ORIGIN=https://<公开域名>`。该变量同时是 OAuth 回调地址的来源；Cookie 的 `Secure` 标志由它推导（`THREADBASE_COOKIE_SECURE` 可覆盖）。

## 技术栈

- 后端：Rust、axum、sqlx/SQLite、`agent-client-protocol` 5.1、`yrs`/`y-sync`、`rust-embed`。
- 前端：Vite 8、React 19、TypeScript strict、Mantine 9、TanStack Query、Zustand、CodeMirror 6。
- 分发：单二进制，内嵌 web 资源；macOS 与 Linux 优先。



## 代码结构

```
crates/shared    wire 类型与领域模型（ts-rs 直接生成 web/src/api/types）
crates/core      纯领域策略：状态机、投递决策、权限
crates/workspace 路径约束、git worktree、文件浏览
crates/draft     Yjs 草稿文档
crates/agent     ACP harness 与 adapter profile
crates/server    HTTP/WS 服务
```

server 按职责分模块：

| 模块 | 职责 |
| --- | --- |
| `app` | 进程级 `AppState`，只负责装配 |
| `projection` | 线程状态读模型与 WebSocket 广播 |
| `threads` | 线程生命周期、worktree、成员与 DTO |
| `agents/` | `AgentManager` 门面、事件泵、队列 worker、生命周期副作用 |
| `approvals` / `elicitations` | 审批与提问的持久化、状态迁移、规则 |
| `queue` / `comments` / `subagents` / `bindings` | 各自实体的唯一写入点 |
| `inbox` / `notify` | inbox 记录、webhook 与成员通知 |
| `events` | 追加式事件日志（每线程顺序、跨线程并行） |
| `diff` | 基线与审阅；线程列表不再触发任何 git 子进程 |
| `routes_*` | HTTP 路由，只做鉴权、解析和调用领域模块 |

前端入口按域拆分：`web/src/api/` 是 fetch 层，`components/thread/` 是时间线行组件，`components/composer/` 是 Composer 的命令与建议面板。

## 已定的事

- 单机多用户：一个服务进程，多人用浏览器访问同一个页面。
- 后端 Rust，单二进制分发；前端 Vite + React 19 + Mantine 9。
- 首发 agent 为 Codex，通过 ACP v1 接入；同时提供 Claude Code、opencode、pi 的 adapter profile。
- 开源，Apache-2.0；v1 支持 macOS 与 Linux。
- 服务固定绑回环，由外部 HTTPS 隧道（如 Tailscale Funnel）终结 TLS；身份只来自 GitHub OAuth 会话。
- 草稿层用 CRDT；事件日志仍由服务端定序。
- 焦点共享走 presence，可跟随成员；子 agent 用左栏树 + tab，审批只阻塞发起的子 agent。
- UI 交互语义参考 Delta，视觉以 Linear/Apple Notes 与 Mantine 为准。

## 开发命令

```bash
# 并行跑全部质量门（热态约 4s）
scripts/verify.sh

# 单独执行
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
MBX_BLAZE=false cargo nextest run --workspace
pnpm --dir web typecheck
pnpm --dir web build
```

前端没有单独的 lint 目标：v2 前端工具栏只剩 `tsc --noEmit` 与 `vite build`，`web/audit/*.mjs` 是需要浏览器的手工审计脚本。

真实 Codex 契约 smoke（需要登录）：

```bash
MBX_BLAZE=false cargo test -p threadbase --test real_codex -- --ignored --nocapture
```

构建加速：本机使用 `mr-boxington` + blaze 时，`cargo check` 会走 blaze；测试、release 与 fixture 使用 `MBX_BLAZE=false`。

## 打包

```bash
MBX_BLAZE=false cargo build --release -p threadbase
threadbase doctor
threadbase service install --print
```

GitHub Releases 工作流见 `.github/workflows/release.yml`，安装脚本为 `scripts/install.sh`，升级命令为 `threadbase upgrade [--check]`。

## 许可

Apache-2.0，见 [LICENSE](LICENSE)。
