# Threadbase

暂定代号。一个面向小团队的 coding agent 协作工作台：以本地项目目录为一级维度，把 agent 对话、代码变更和审批放进可共享的 thread，队友可以实时观看、发言、接管和评审。

产品形态参考 [Delta](https://delta.dev/)，但不自建 agent runtime：底层复用 Claude Code、Codex、opencode、pi 等已有 agent，通过 ACP 接入。

## 当前状态

v1 已实现并通过验证。包含 Rust 单二进制服务、React 工作台、真实 ACP 会话、SQLite 事件日志、CRDT 草稿、审批/提问、Changes 审阅、评论、presence/跟随、分享与角色、CLI 与发布工作流。

验证证据、版本审计、规格冲突裁决和逐项需求覆盖见 [docs/06-implementation-status.md](docs/06-implementation-status.md)。界面截图见 [docs/screenshots](docs/screenshots)。

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

## 部署模型：本机服务，tailnet 内访问

服务跑在开发机上，只监听回环地址；所有远程访问都由 `tailscale serve` 终结 TLS 并转发到 `127.0.0.1:7777`：

```bash
tailscale serve --bg 443 http://127.0.0.1:7777
```

其他设备访问 `https://<machine>.<tailnet>.ts.net`。Tailscale ACL 决定谁能连上机器，应用身份决定谁能读写哪条 thread。

### 登录：只有 Tailscale 身份，没有密码

- 应用不保存密码，也没有登录表单、注册或会话 Cookie。
- 每个请求都带 `tailscale serve` 转发的身份头（`Tailscale-User-Login`、`-Name`、`-Profile-Pic`），服务只信任来自回环 peer 的这些头。
- 首次出现的 tailnet 身份自动建号，并成为该实例的管理员；之后出现的身份都是普通成员。
- 用 `THREADBASE_TAILSCALE_ADMIN` 指定管理员登录名（逗号分隔），可覆盖自动提升：

```bash
THREADBASE_TAILSCALE_ADMIN=alice@example.com,bob@example.com cargo run -p threadbase
```

- 想撤销某人访问，改 Tailscale ACL 或从工作区移除成员；想重置本机开发环境，直接删 `threadbase.db`。

### 一键部署（systemd）

在 Linux 主机上一条命令把**当前这个二进制**配置成 systemd user 服务：

```bash
threadbase install --admin you@example.com
```

它会：

- 写 `~/.config/systemd/user/threadbase.service`，`ExecStart` 就是当前进程的可执行文件路径（不拷贝、不构建、不下载）
- `Environment` 里带上绑定地址、数据目录、`THREADBASE_PUBLIC_ORIGIN`（从 `tailscale status` 推导）与管理员登录名，以及去重后的 `PATH`（agent CLI 常在 mise/nvm 路径下）
- `loginctl enable-linger`，让服务在登出后仍在跑
- `systemctl --user enable --now`
- `tailscale serve --bg --https=8443 http://127.0.0.1:7777`
- 轮询 `/api/health` 确认真的起来了

所以要升级，替换这个二进制再跑一次 `threadbase install`（或 `threadbase upgrade` 后 `systemctl --user restart threadbase`）即可。

常用选项：

```bash
threadbase install --print                  # 只打印 unit 与命令，不改动系统
threadbase install --port 7788 --https-port 9443 --unit threadbase-dev
threadbase install --no-serve --no-start     # 只写 unit
threadbase install --uninstall              # 停用并删除 unit、关掉 serve（保留数据与二进制）
```

unit 名默认 `threadbase`；`--unit` 可在同一台机器上跑多个实例。日志：`journalctl --user -u threadbase -f`。

### 本机开发

`pnpm --dir web dev` 的请求来自回环但没有 Tailscale 身份头，需要显式打开开发身份：

```bash
THREADBASE_DEV_IDENTITY=dev@example.com cargo run -p threadbase
```

开发身份只在「回环 peer + 无 `X-Forwarded-*` / `Tailscale-*` 头」时生效，因此经 `tailscale serve` 进来的请求永远不会命中它。不要在生产实例上设置该变量。

### CSRF 与 Cookie

写操作要求 `Origin` 与 `Host` 一致（或等于 `THREADBASE_PUBLIC_ORIGIN`）并带 `X-Requested-With: threadbase`。`tailscale serve` 会保留浏览器的 Host，因此通常无需额外配置；如果前面还有别的反代改写了 Host，就设置 `THREADBASE_PUBLIC_ORIGIN=https://<machine>.<tailnet>.ts.net`。

## 技术栈

- 后端：Rust、axum、sqlx/SQLite、`agent-client-protocol` 5.1、`yrs`/`y-sync`、`rust-embed`。
- 前端：Vite 8、React 19、TypeScript strict、Mantine 9、TanStack Query、Zustand、CodeMirror 6。
- 分发：单二进制，内嵌 web 资源；macOS 与 Linux 优先。

详细版本与偏差见 [docs/06](docs/06-implementation-status.md)。

## 文档

| 文档 | 内容 |
| --- | --- |
| [docs/01-requirements.md](docs/01-requirements.md) | 需求文档：产品定义、领域模型、关键流程、协作语义、功能清单、非功能需求 |
| [docs/02-open-decisions.md](docs/02-open-decisions.md) | 决策记录：部署、技术栈、CRDT、Tailscale、子 agent、steering、评论等 19 项 |
| [docs/03-tech-stack.md](docs/03-tech-stack.md) | 技术栈：选型矩阵、架构、事件模型、存储、实时层、质量 |
| [docs/04-subagents-and-questions.md](docs/04-subagents-and-questions.md) | 子 agent、队列、审批路由、steering、提问与评论 |
| [docs/05-ui-spec.md](docs/05-ui-spec.md) | UI 规格：交互参考 Delta，视觉以 Linear/Apple Notes 与 Mantine 为准；外壳、左栏树、时间线、Composer、Changes、键位 |
| [docs/06-implementation-status.md](docs/06-implementation-status.md) | 实现状态、版本审计、规格冲突裁决、验证证据 |
| [docs/07-state-machines.md](docs/07-state-machines.md) | 隐式状态机审计（已被 08 取代，仅作历史输入） |
| [docs/08-state-model-decision.md](docs/08-state-model-decision.md) | 统一显式状态模型决策与落地记录 |
| [docs/09-audit-findings.md](docs/09-audit-findings.md) | 文档-实现差异审计与修正清单 |
| [docs/research/delta-ui.md](docs/research/delta-ui.md) | Delta 界面与交互调研 |

## 已定的事

- 单机多用户：一个服务进程，多人用浏览器访问同一个页面。
- 后端 Rust，单二进制分发；前端 Vite + React 19 + Mantine 9。
- 首发 agent 为 Codex，通过 ACP v1 接入；同时提供 Claude Code、opencode、pi 的 adapter profile。
- 开源，Apache-2.0；v1 支持 macOS 与 Linux。
- 只通过 Tailscale 访问；服务固定绑回环，由 `tailscale serve` 终结 TLS。
- 草稿层用 CRDT；事件日志仍由服务端定序。
- 焦点共享走 presence，可跟随成员；子 agent 用左栏树 + tab，审批只阻塞发起的子 agent。
- UI 交互语义参考 Delta，视觉以 Linear/Apple Notes 与 Mantine 为准，只有文档列出的五处刻意偏离。

## 开发命令

```bash
# 并行跑全部质量门（热态约 4s；--with-e2e 追加浏览器 e2e）
scripts/verify.sh
scripts/verify.sh --with-e2e

# 单独执行
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
MBX_BLAZE=false cargo nextest run --workspace
pnpm --dir web typecheck
pnpm --dir web lint
pnpm --dir web build
pnpm --dir web exec playwright test
```

真实 Codex 契约 smoke（需要登录）：

```bash
MBX_BLAZE=false cargo test -p threadbase --test real_codex -- --ignored --nocapture
```

构建加速：本机使用 `mr-boxington` + blaze 时，`cargo check` 会走 blaze；测试、release 与 fixture 使用 `MBX_BLAZE=false`，原因和配置见 [docs/06](docs/06-implementation-status.md) §6。

## 打包

```bash
MBX_BLAZE=false cargo build --release -p threadbase
threadbase doctor
threadbase service install --print
```

GitHub Releases 工作流见 `.github/workflows/release.yml`，安装脚本为 `scripts/install.sh`，升级命令为 `threadbase upgrade [--check]`。

## 许可

Apache-2.0，见 [LICENSE](LICENSE)。
