# 实现状态与决策记录

配套文档：[01-requirements.md](01-requirements.md)、[02-open-decisions.md](02-open-decisions.md)、[03-tech-stack.md](03-tech-stack.md)、[04-subagents-and-questions.md](04-subagents-and-questions.md)、[05-ui-spec.md](05-ui-spec.md)。

本文件记录实现阶段的事实：实际版本、与规格的偏差、需求覆盖情况和验证证据。规格冲突在这里给出裁决，不再散落到代码里。

---

## 1. 版本审计（2026-11-09）

文档采样的版本部分已过时。下表是实现实际采用并通过构建的版本；未列出的依赖按 `Cargo.lock` / `pnpm-lock.yaml` 为准。

| 依赖 | 文档版本 | 实际版本 | 原因 |
| --- | --- | --- | --- |
| `agent-client-protocol` | 5.0 | **5.1.0** | 当前稳定版；5.x 为同一 semver 线，启用 `process` 与 `unstable_subagents` |
| `sqlx` | 0.9 | **0.8.6** | `tower-sessions-sqlx-store 0.15.1` 固定依赖 sqlx 0.8，避免双版本与 SQLite 驱动冲突 |
| `yrs` | 0.28 | **0.17.4** | `y-sync 0.4.0` 固定依赖 yrs 0.17；y-sync 是当前唯一的 Yjs 服务端同步实现 |
| `y-sync` | 0.4 | **0.4.0** | 与上一条配套 |
| `notify` | 8.2 | **8.2** | 9.0 仍为 RC，不用 |
| `ulid` | 未指定 | **1.2** | 2/3 有 API 变化，1.x 足够 |
| `ts-rs` | 12 | **12.0.1** | 启用 `no-serde-warnings` 与 `serde-json-impl` |
| `Vite` | 8 | **8.3.7** | 当前稳定 |
| `React` | 19 | **19.3.1** | 当前稳定 |
| `TypeScript` | strict | **5.9.3** | TS 7 是新的原生工具链，生态兼容性未验证；5.9 为稳定选择 |
| `React Router` | 8 | **7.18.5** | 7 是当前稳定大版本；data router 能力满足深链接需求 |
| `Tailwind` | v4 | **4.3.5** | 当前稳定 |
| `shiki` | 4 | **4.6.0** | 当前稳定 |
| `pnpm` | 未指定 | **10.34.9** | 通过 `packageManager` 固定 |
| `tower-sessions` | 0.15 | **未使用** | 会话写入自建 `auth_sessions` 表，才能按用户批量撤销并保留审计；tower-sessions 的 store 表不提供该语义 |

宿主环境（本机）：Rust 1.99.1、Node 26.8.1、npm 12.0.2、Tailscale 已运行；Codex、Claude Code、opencode、pi 均已安装。`node_modules` 与 `target` 不提交。

### 1.1 ACP 版本差异

- 5.1.0 采用 `Client.v1().connect_with(...)` + 回调模型，而非文档中的 `Client` trait 直接实现；`Harness` trait 仍是边界，SDK 类型不会进入事件模型。
- 子 agent 能力在 `unstable_subagents` feature 后；该能力属于 RFD，按文档要求按 adapter 单独开关，默认退化。
- `terminal` 仍声明 `false`；v1 不引入 PTY。

---

## 2. 规格冲突裁决

| 冲突 | 裁决 | 依据 |
| --- | --- | --- |
| 05 描述 `!` 内联终端与 Terminals picker，01 非目标与 03 技术栈声明无 PTY / `terminal: false` | v1 不做可交互终端；提供“agent 命令输出”只读面板，终端类工具调用可展开查看完整输出 | 安全边界、单二进制、ACP 能力一致 |
| P2 列“终端面板、文件浏览”，非目标列“不做 IDE” | 文件浏览只读；终端面板只读展示 agent 执行的命令与输出；不提供 shell | 同上 |
| 01 §9 有 Reviews / Merge 相关 thread，02 已定 14 明确不做 | 保留单 thread 内的 Changes 审阅与“合回主工作区”，不做 Reviews/Merge 两类 thread，不做 PR merge 流程 | 02 已定 14 |
| 分享档位在 01 为三档，02 已定 15 改为两档 | 只做“仅邀请 / 整个工作区” | 02 已定 15、17 |
| 通知为 P2（浏览器 + Webhook） | 浏览器通知 + 应用内收件箱；Webhook 做最小 POST（URL + 事件类型），契约见本文件 §6 | P2 完整覆盖，不扩成通知平台 |

---

## 3. 需求覆盖

状态：`未开始` / `进行中` / `已实现` / `已验证`。只有同时有代码路径和验证证据才标“已验证”。

| 需求 | 状态 | 代码路径 | 验证 |
| --- | --- | --- | --- |
| F1 添加并打开 project | 进行中 | 认证与会话已完成；project API 待接入 | 22 个服务端认证测试通过 |
| F2 新建 thread / worktree / agent | 未开始 | — | — |
| F3 对话与工具调用 | 未开始 | — | — |
| F4 审批 | 未开始 | — | — |
| F5 分享与加入 | 未开始 | — | — |
| F6 多人实时 | 未开始 | — | — |
| F7 变更审阅 | 未开始 | — | — |
| F8 断线与恢复 | 未开始 | — | — |
| F9 thread 生命周期 | 未开始 | — | — |
| F10 project 概览 | 未开始 | — | — |
| F11 焦点共享与跟随 | 未开始 | — | — |
| F12 子 agent 展示 | 未开始 | — | — |
| F13 提问与回答 | 未开始 | — | — |
| F14 队列与中途插话 | 未开始 | — | — |
| F15 评论与标注 | 未开始 | — | — |

---

## 4. 代码结构

```
crates/
  shared/     DTO、事件、错误码、ts-rs 导出
  core/       角色/审批策略、投递决策、事件归约（纯逻辑）
  draft/      Y.Doc 草稿块、发送取块、快照
  agent/      Harness trait、adapter profile、ACP 归一化
  workspace/  路径约束、git worktree、diff
  server/     axum、SQLite、REST、WS、CLI、静态资源
web/          React 19 + Vite + Tailwind v4
```

依赖方向：`server → core → agent | workspace | draft`；`core` 不依赖 axum、sqlx、ACP。

---

## 5. 当前验证证据

截至最近一次检查：

```
cargo check --workspace                    pass
cargo test --workspace                     pass（全部测试，0 失败）
cargo clippy --workspace --all-targets -D warnings  pass
pnpm --dir web typecheck                   pass
pnpm --dir web lint                        pass
pnpm --dir web build                       pass
```

已覆盖：
- SQLite 迁移与 100k 事件窗口加载；并发 seq 单调、afterSeq 重放、resync、快照恢复。
- argon2id 密码、会话签发/撤销、登录限流、CSRF（自定义头 + Origin）、Tailscale 身份头仅回环可信、首次 setup 仅本地。
- 路径穿越（词法 + 真实路径 + symlink）、git worktree 创建/清理、角色矩阵（owner/editor/viewer/admin）。

尚未完成：project/thread REST、WS、ACP 会话、前端业务页面、浏览器 e2e、release 打包。

---

## 6. 通知 Webhook 契约（最小实现）

- 配置：`webhook_url`，存 `settings`，仅管理员可改。
- 事件：`approval.pending`、`elicitation.pending`、`comment.mention`、`thread.error`。
- 请求：`POST`，`Content-Type: application/json`，body `{ event, threadId, threadTitle, actor, summary, createdAt }`。
- 超时 5 秒，失败只记日志，不阻塞 thread。
- 不重试、不做签名队列；后续如需可靠性再升级。

---

## 7. 下一步

按 goal 任务树推进：事件存储 → 认证与安全 → workspace → ACP harness → REST/WS → 前端时间线与 Composer → 审批/评论/分享 → 打包与全量验证。
