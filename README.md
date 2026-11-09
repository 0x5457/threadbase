# Threadbase

暂定代号。一个面向小团队的 coding agent 协作工作台：以本地项目目录为一级维度，把 agent 对话、代码变更和审批放进可共享的 thread，队友可以实时观看、发言、接管和评审。

产品形态参考 [Delta](https://delta.dev/)，但不自建 agent runtime：底层复用 Claude Code、Codex、opencode、pi 等已有 agent，通过 ACP 接入。

## 当前状态

需求与 UI 规格阶段。只有文档，没有代码。技术栈已定，剩三处产品口径待确认。

## 文档

| 文档 | 内容 |
| --- | --- |
| [docs/01-requirements.md](docs/01-requirements.md) | 需求文档 v0.1：产品定义、领域模型、关键流程、协作语义、界面要求、功能清单、非功能需求、里程碑 |
| [docs/02-open-decisions.md](docs/02-open-decisions.md) | 决策记录 v0.6：已定 19 项（含 CRDT 草稿层、Tailscale 身份与访问、GitHub Releases 分发、五处刻意偏离），无待定 |
| [docs/03-tech-stack.md](docs/03-tech-stack.md) | 技术栈 v0.3：语言选型矩阵、axum + sqlx + ACP Rust SDK、per-author 草稿模型、presence、Cargo workspace、分发与质量 |
| [docs/04-subagents-and-questions.md](docs/04-subagents-and-questions.md) | 子 agent、队列与标注：协议现状、树形展示、审批路由、steering、评论与 elicitation |
| [docs/05-ui-spec.md](docs/05-ui-spec.md) | UI 规格 v0.1（对齐 Delta v0.24.0）：外壳、左栏树、时间线密度、Composer、Changes、协作、审批、键位、状态矩阵 |
| [docs/research/delta-ui.md](docs/research/delta-ui.md) | Delta 的界面与交互调研：信息架构、逐屏规格、交互规则、视觉观察、可借鉴清单、未验证项 |

## 已定的事

- 单机多用户：一个服务进程，多人用浏览器访问同一个页面。
- 后端 Rust，单二进制分发；前端 Vite + React 19 + Tailwind v4 + shadcn/ui。
- 首发 agent 为 Codex，通过 ACP v1 接入。
- 开源，Apache-2.0；v1 支持 macOS 与 Linux（同一套 Rust 代码）。
- 只通过 Tailscale 访问，不暴露公网；服务固定绑回环，由 `tailscale serve` 终结 TLS，优先用 Tailscale 身份免密登录。
- 首发 macOS + Linux，通过 GitHub Releases 分发。
- 草稿层用 CRDT（DeltaDB 同为 CRDT）；事件日志仍由服务端定序。
- 焦点共享走 presence，可跟随成员；子 agent 用左栏树 + tab，审批只阻塞发起的子 agent。
- UI 以 Delta v0.24.0 为准，只有五处刻意偏离：审批 UI、project 一级维度、子 thread 无 composer、无 Reviews/Merge thread、分享两档。

## 下一步

1. 先做规格：视觉 token、组件清单、M1 验收脚本、ACP spike 清单（见 `02` 末尾）。
2. 验证 Tailscale 身份头（一个本地探针页，不写业务代码）。
3. 实现等明确指令后再开始。
