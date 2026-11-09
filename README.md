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
MBX_BLAZE=false cargo run -p threadbase-server
```

首次打开页面创建本地管理员。服务启动后运行 `threadbase doctor` 检查 Node、git、数据库和 Codex/Claude/opencode/pi adapter。

开发前端时另开一个终端：

```bash
pnpm --dir web dev   # http://127.0.0.1:5173，/api 与 /ws 代理到 7777
```

真实 Codex 需要安装 `codex-acp` 并登录 Codex；服务会自动把本机 `codex` 路径写入 `CODEX_PATH`。没有 adapter 时 `doctor` 会明确报告，UI 会把 thread 标记为错误而不是卡在运行中。

数据目录默认是平台数据目录下的 `threadbase/`（Linux 为 `~/.local/share/threadbase/`），可用 `THREADBASE_DATA_DIR` 覆盖。服务固定绑定回环地址，`THREADBASE_BIND` 只接受回环地址。

## 远程访问

只支持 tailnet，不暴露公网：

```bash
tailscale serve --bg 443 http://127.0.0.1:7777
```

浏览器访问 `https://<machine>.<tailnet>.ts.net`。Tailscale ACL 决定谁能连上机器，应用账号决定谁能读写哪条 thread。不要把端口转发到公网。

## 技术栈

- 后端：Rust、axum、sqlx/SQLite、`agent-client-protocol` 5.1、`yrs`/`y-sync`、`rust-embed`。
- 前端：Vite 8、React 19、TypeScript strict、Tailwind v4、shadcn/ui 风格组件、TanStack Query、Zustand、CodeMirror 6。
- 分发：单二进制，内嵌 web 资源；macOS 与 Linux 优先。

详细版本与偏差见 [docs/06](docs/06-implementation-status.md)。

## 文档

| 文档 | 内容 |
| --- | --- |
| [docs/01-requirements.md](docs/01-requirements.md) | 需求文档：产品定义、领域模型、关键流程、协作语义、功能清单、非功能需求 |
| [docs/02-open-decisions.md](docs/02-open-decisions.md) | 决策记录：部署、技术栈、CRDT、Tailscale、子 agent、steering、评论等 19 项 |
| [docs/03-tech-stack.md](docs/03-tech-stack.md) | 技术栈：选型矩阵、架构、事件模型、存储、实时层、质量 |
| [docs/04-subagents-and-questions.md](docs/04-subagents-and-questions.md) | 子 agent、队列、审批路由、steering、提问与评论 |
| [docs/05-ui-spec.md](docs/05-ui-spec.md) | UI 规格（对齐 Delta v0.24.0）：外壳、左栏树、时间线、Composer、Changes、键位 |
| [docs/06-implementation-status.md](docs/06-implementation-status.md) | 实现状态、版本审计、规格冲突裁决、验证证据 |
| [docs/research/delta-ui.md](docs/research/delta-ui.md) | Delta 界面与交互调研 |

## 已定的事

- 单机多用户：一个服务进程，多人用浏览器访问同一个页面。
- 后端 Rust，单二进制分发；前端 Vite + React 19 + Tailwind v4。
- 首发 agent 为 Codex，通过 ACP v1 接入；同时提供 Claude Code、opencode、pi 的 adapter profile。
- 开源，Apache-2.0；v1 支持 macOS 与 Linux。
- 只通过 Tailscale 访问；服务固定绑回环，由 `tailscale serve` 终结 TLS。
- 草稿层用 CRDT；事件日志仍由服务端定序。
- 焦点共享走 presence，可跟随成员；子 agent 用左栏树 + tab，审批只阻塞发起的子 agent。
- UI 以 Delta v0.24.0 为准，只有文档列出的五处刻意偏离。

## 开发命令

```bash
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
MBX_BLAZE=false cargo test -p threadbase-server --test real_codex -- --ignored --nocapture
```

构建加速：本机使用 `mr-boxington` + blaze 时，`cargo check` 会走 blaze；测试、release 与 fixture 使用 `MBX_BLAZE=false`，原因和配置见 [docs/06](docs/06-implementation-status.md) §6。

## 打包

```bash
MBX_BLAZE=false cargo build --release -p threadbase-server
threadbase doctor
threadbase service install --print
```

GitHub Releases 工作流见 `.github/workflows/release.yml`，安装脚本为 `scripts/install.sh`，升级命令为 `threadbase upgrade [--check]`。

## 许可

Apache-2.0，见 [LICENSE](LICENSE)。
