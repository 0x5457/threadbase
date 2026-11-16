# Threadbase

暂定代号。一个面向小团队的 coding agent 协作工作台：以本地项目目录为一级维度，把 agent 对话、代码变更和审批放进可共享的 thread，队友可以实时观看、发言、接管和评审。

产品形态参考 [Delta](https://delta.dev/)，但不自建 agent runtime：底层复用 Claude Code、Codex、opencode、pi 等已有 agent，通过 ACP 接入。

## 当前状态

v1 已实现并通过验证。包含 Rust 单二进制服务、React 工作台、真实 ACP 会话、SQLite 事件日志、CRDT 草稿、审批/提问、Changes 审阅、评论、图片/文件附件与大文本粘贴、presence/跟随、线程关注/静音、全文搜索、线程历史与行级 diff 评论、Worktree 初始化钩子、PWA/Web Push、CLI 与发布工作流。

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

### 通知、关注与 Agent 生命周期

- 每个 thread 的创建者自动成为 watcher；之后发消息、评论、审批或回答问题的人也会自动加入。`GET/POST /api/threads/{id}/watch` 可以显式关注、静音或取消关注；`ThreadMenu` 里的 “Mute notifications” 只影响通知，不影响读写。
- approval、elicitation、@mention、turn 完成和错误只通知该 thread 的 **未静音 watcher**，不再给全工作区每人写一条 inbox。完成通知会跳过当前正聚焦该 thread 的人；错误通知不会。
- Inbox 现在有分页（`/api/inbox?before=<id>&limit=50`），`unread` 是账号的完整未读数而不是当前页的条数；新通知、标记已读和审批/提问解决都会通过现有 `/ws` 推送 `inbox_count`，只发给对应用户的 socket。
- Agent 采用懒启动：新建 thread 不再立刻拉起 ACP 进程，首次发送/排队输入时才启动。空闲 `THREADBASE_IDLE_SESSION_SECS`（默认 900，0 关闭）后会关闭进程；binding 里的 ACP session id 会在下次 prompt 时自动 resume，失败则发 context reset notice。
- `THREADBASE_MAX_ACTIVE_TURNS`（默认 4，0 不限）限制所有 thread 同时运行的 turn。达到上限时直接发送会转成 queued input，队列 worker 等到 permit 释放后继续，不会把 HTTP 请求挂住。
- 删除 thread/project、归档 thread 之前会先停掉对应的 agent 进程，避免 agent 在已删除的 worktree 里继续写。
- PWA：`/manifest.webmanifest` 与 `sw.js` 让页面可以安装；设置页的 **Push notifications** 通过 VAPID + `push_subscriptions` 接收审批、@mention、turn 完成和错误通知。VAPID 密钥首次使用时自动生成并保存在 `push_config`，不需要手工配置。Push 只在 HTTPS（或 localhost）安全上下文可用；iOS 需要先“添加到主屏幕”。关掉页面后仍能收到通知，打开且正聚焦同一 thread 时 service worker 会抑制重复通知。

### Worktree 初始化钩子

项目设置里的 **Setup command** 会在每个新 worktree（包括 fork）创建后、agent 启动前执行一次，适合做 `pnpm install`、复制 `.env`、启动依赖服务等：

- 状态持久化在 `threads.setup_status`（`none/pending/running/succeeded/failed`）与 `setup_error`；对话框和 thread 顶部的 banner 会显示进度，失败时可以点 **Retry setup**。
- setup 期间不能发 prompt，后端 `ensure_started` 会拒绝，避免 agent 在依赖还没装好的 worktree 里开始工作。
- 输出以 `AgentLog` 写进事件流（最多保留尾部 32 KB），最终结果写一条 `SystemNotice`；失败也会向 watcher 发 inbox 通知。
- setup 被服务重启打断时，启动阶段会把它标成 `failed` 并提示重跑，而不是永久卡在 running。
- 超时由 `THREADBASE_SETUP_TIMEOUT_SECS` 控制（默认 900 秒）。

### 全文搜索

`GET /api/search?q=&project_id=&limit=` 使用 SQLite FTS5 搜索 thread 标题、用户 prompt、agent 消息和评论。`Cmd/Ctrl+K` 打开 Spotlight 后输入至少 2 个字符，结果按 Messages / Comments / Thread titles 分组，点击直接打开 thread。

- 索引由 `EventStore::append` 增量维护，thread 重命名、删除和评论删除会同步清理；已有数据库第一次启动时后台回填。
- 需要强制重建时运行 `threadbase reindex`。
- 查询按空白分词并全部加引号，只做 AND 匹配，不会把 `-`、`:`、`*` 当成 FTS 运算符。

### 线程历史与行级评论

- 右栏的 `History` 列出 worktree 里自 base 之后的提交。每轮 prompt 前 `snapshot_worktree` 会写一个 `Threadbase turn snapshot`，面板把快照映射回产生它的那次 turn，显示 prompt 摘要、时间和增删行数。点击一行会在 `Changes` 里打开该提交**自身相对父提交**的 diff，`Current` 回到工作区。
- `GET /api/threads/{id}/commits` 是历史列表；`GET /api/threads/{id}/commits/{sha}` 与 `.../file?path=` 返回提交级 diff。sha 只接受 7–64 位十六进制字符。
- Changes 面板里可以点击行号写行级评论，锚点保存 path、hunkId、行号、side 和 contextHash。hunk 内容变化时服务端按 contextHash 重新定位；找不到则退化为文件级锚点（`anchorChanged: true`），评论仍显示在文件顶部。
- 行级评论只在 base-branch 基线上创建和展示，因为服务端锚点校正是相对 base 的；其他基线仍可查看 diff。
- History 读的是 worktree 的 git 历史：删除 thread 会一并删除它的 worktree 和分支，因此这些提交最终会被 git 清理。需要长期审计时，应把提交信息持久化进数据库，而不是只依赖 worktree。

### 编辑消息与回退

- 用户消息在桌面悬停时出现铅笔按钮，手机长按打开 ActionSheet；保存走 `POST /api/threads/{id}/messages/{message_id}/edit`，不会新增一条“请忽略上一条”的补丁消息。
- 保存会删除该消息之后的全部事件，同时清理排队输入、待处理的审批/提问、相关评论通知和子代理行；`ThreadRewound` 通过 `/ws` 通知所有打开的窗口丢弃本地日志并重新订阅。
- ACP adapter 的会话无法回退，所以 Threadbase 会清掉 binding 里的 session id，在新会话里只发送这一条编辑后的 prompt。编辑点之前的轮次不会进入新会话上下文，worktree 文件也不会回滚——对话框会明确写出这两点。
- 因为 seq 不复用，回退后的日志会从较高的序号开始；`events_page.hasEarlier` 单独判断是否真的有更早的事件，避免出现空的 “Load earlier”。

### 应用变更前的检查

`Apply changes` 不再直接 `git apply` + `commit -a`：

- 要求项目工作区干净，并且当前 checkout 的分支就是 thread 的 base branch（或项目默认分支）；
- 如果 base branch 在 thread 启动后前进过，响应会带 `baseDrift: true`，UI 会提示复查；
- 项目设置里的 **Verify command** 会先在 thread worktree 里执行，退出码非 0 时拒绝应用并返回输出尾部；命令超时 5 分钟；
- commit 失败会自动 `reset --hard` + `clean -fd` 回到应用前的 HEAD，避免留下半应用状态。

顶部菜单的 “Apply changes” 仍然是本地 worktree 模型：不切分支、不 push、不创建 PR。

### 事件日志维护

每个 token chunk 都写一行事件，长期使用后 `events` 表会膨胀（本机实测 181 MB）。`compact` 用和冷加载相同的折叠规则重写日志：流式思考/消息合并成一条，已完成消息只保留 `message_completed`，usage/plan 历史全部保留，重放出来的 transcript 不变。

```bash
threadbase compact --dry-run   # 只报告每个线程会删除/折叠多少行
threadbase compact             # 在线可跑；只是普通 WAL 事务
threadbase compact --vacuum    # 重建数据库文件回收磁盘
```

`--vacuum` 需要独占锁，服务正在写时会自动跳过并提示；要稳定回收磁盘，先 `systemctl --user stop threadbase`，跑完再启动。折叠可以反复跑，第二次之后就没有可删的行。

### 附件与大文本粘贴

- Composer 左下角的回形针可以选多个本地文件；也可以直接把文件拖进 Composer，或从剪贴板粘贴图片/文件。
- 图片（PNG / JPEG / WebP / GIF，单张 8 MB）在对话里内联显示，点开灯箱预览。
- 其它文件（单个 25 MB）作为附件保存到数据目录，消息里显示文件名/大小，点击强制下载；一条消息最多 8 个附件。
- 服务端不会把非图片文件当 HTML/SVG 内联执行：下载时使用 `application/octet-stream`、`Content-Disposition: attachment` 和 `nosniff`。
- 粘贴超过 **8000 字符** 的纯文本时，Composer 自动把它保存成 `pasted-<timestamp>.txt` 附件，不再把整段文字塞进编辑器；上传失败会回退为普通文本插入，避免丢内容。
- 给 agent 的 prompt：图片走原生 image block；文本文件 ≤32 KB 内联到 prompt，较大的文本给前 4 KB 预览 + 绝对路径；二进制文件只给路径和元数据，让 agent 用自己的文件工具读取。
- 存储位置 `<数据目录>/attachments/<thread>/`，不写入项目 worktree，也不会自动提交进 git；附件按现有事件模型随消息保留。

### 草稿标记

- 共享 composer 草稿一旦有文字，左栏对应 thread 行会显示一个铅笔小标志；Sessions 列表里对应 thread 行也会显示 `draft`。
- 服务端只在空 ↔ 非空转换时更新 `threads.has_draft`，不会每个按键写库；发送或清空草稿后标记消失。
- 启动时会从已持久化的 `draft_updates` / `draft_snapshots` 回填旧草稿的标记，所以升级前的草稿也不会丢标志。

### 网页终端（默认关闭）

`THREADBASE_TERMINAL=1` 才挂载终端路由，且默认**只有管理员能驱动**：其他 `member` 账号打开同一个终端只能看（tmux 只读客户端，服务端不接收其按键），`read_only` 账号连入口都没有。`THREADBASE_TERMINAL_DRIVERS=0x5459,alice` 可以给指定 GitHub 登录（逗号分隔，大小写不敏感）驱动权限，不必把整个账号提成管理员。关掉这个功能时相关路由返回 404，不暴露存在性。

```bash
THREADBASE_TERMINAL=1 cargo run -p threadbase
# 浏览器里按 Ctrl+` 或点左栏底部的终端图标
```

- **形态**：底部抽屉（`Ctrl+`` 开关，拖上边缘改高度，`⤢` 全屏）。每个 thread 一个 shell，cwd 跟着 agent 走：有 worktree 的 thread 在 worktree 里，`worktree: false` 的 thread 在项目目录里；未选中 thread 时开的是 `$HOME` 里的全局 shell（`h-<user>`）。
- **持久化在 tmux 内存里，不落盘**：会话名 `t-<thread>` / `h-<user>`，挂在私有 socket `threadbase` 上（`THREADBASE_TERMINAL_SOCKET` 可换）。关抽屉、刷新、重启服务都只是 detach，shell 继续跑；`history-limit=10000` 的 scrollback 也只存在 tmux server 内存里。
- **只读由 tmux 执行**：driver 用 `tmux attach`，其他人 `tmux attach -r`；`window-size manual` + `resize-window` 让窗口尺寸跟 driver 走，围观者小屏是裁切而不是让 driver 的终端换行。
- **鉴权**：WebSocket 不能带自定义头，所以升级用一次性 ticket（`POST /api/terminal/sessions`，受 CSRF 中间件保护，30 秒过期、用户与会话绑定）。升级时还要求会话 Cookie 与当前用户行仍然有效。
- **不记日志**：终端内容不写入磁盘、也不进事件日志；需要复盘时只能靠 tmux 里还活着的 scrollback。想彻底停掉就 `tmux -L threadbase kill-server`。
- **退出即提示**：shell 退出时客户端收到 EOF，抽屉显示 `The shell exited (code)` 和 `New session`，不会留下一个死掉的 pane。
- **在场感**：开着终端的人会在其他人的 composer 工具栏里显示一个终端图标（`Panel::Terminal` 走现有 presence 通道）；关掉抽屉但 shell 还活着时，状态栏留一个 `terminal` 小按钮。两者点击都能回到终端。
- **前端**：xterm.js 懒加载；按键条补上浏览器吞掉的键（Ctrl+C/D/L/Z/R、Esc、Tab、方向键）；`Ctrl+Shift+C/V` 复制粘贴，多行粘贴先确认。
- **手机**：抽屉是全屏，点终端即聚焦并唤起软键盘（helper textarea 被放到视口内 1×1、16px 字号，否则 iOS 不给键盘还会连带缩放页面）；键盘弹出时 `interactive-widget=resizes-content` 会让布局收缩，ResizeObserver 重新 fit 并把新尺寸同步给 tmux。ticket 仍支持 `watchOnly`（服务端拒绝 drive），只是界面不再强制手机只读。

### 宿主机 tmux（第二个开关）

上面的 workbench 终端只碰私有 socket。想在 Threadbase 里看并接管这台机器上原本的 tmux 会话，再打开一个开关：

```bash
THREADBASE_TERMINAL=1 \
THREADBASE_TERMINAL_HOST=1 \
THREADBASE_TERMINAL_HOST_SOCKETS=default,work \
cargo run -p threadbase
```

- `THREADBASE_TERMINAL_HOST_SOCKETS` 是 `tmux -L` 的 socket 名，逗号分隔；不写默认只有 `default`（普通 `tmux` 用的那个）。私有 `THREADBASE_TERMINAL_SOCKET` 永远会被排除，否则可以从宿主机入口绕过 workbench 的只读策略。
- 顶部栏的 Sessions 图标、左栏 Sessions、或命令面板的 “Go to Sessions” 打开统一列表；点任意一行就在底部抽屉里 attach。抽屉标题是 `tmux · <name>`，不会杀掉宿主机会话，关闭只是 detach。终端抽屉头部的列表按钮打开快速切换器。
- 权限：所有 `member` 和 admin 都能看、能接管；`read_only` 和待接受邀请仍然拿不到 shell。workbench 线程终端的规则不变（member 默认只看）。
- 目标一律用 tmux 的 `$N` session id，不用名字；名字可以包含 `:`、`.` 等 target 语法字符，只用于展示。
- **单浏览器 driver**：同一时刻只有一个浏览器持有键盘，其他浏览器是 watching，点提示条里的 **Take over** 就可以在没有重连的情况下接管；前一个 driver 的输入会立即被服务端丢掉。抽屉头部显示当前 viewers 头像和谁在 driving。本机已经 attach 的 tmux client 不受这个仲裁控制，仍然可以输入。
- **窗口尺寸**：tmux 默认 `window-size latest`，浏览器抽屉的 pty 尺寸会成为最新 client 的尺寸，可能让你本机终端短暂重排。介意的话在 `~/.tmux.conf` 用 `set -g window-size largest` 或 `manual`；Threadbase 不会改宿主机的 tmux 全局选项。
- **安全边界**：这个开关等价于“把服务账号在这台机器上的交互式 shell，以可接管方式给所有 member”。只对可信团队打开，隧道入口建议再加 Tailscale ACL；不想给了就删掉 `THREADBASE_TERMINAL_HOST` 并重启。宿主机会话的内容不写日志、不落盘。

### Sessions 视图

`sessions` 页面把 thread 和 tmux session 放在同一个“现在能打开什么”的列表里：

- 左边栏的 **Sessions**、顶部栏的列表图标或命令面板都能打开。
- 支持搜索、`All / Threads / Tmux` 过滤；排序是 pinned → 正在运行 → 最近活动。
- thread 行打开 transcript；如果该 thread 的私有 workbench shell 还活着，行内会显示 shell 状态和一个终端按钮。member 默认只能 watch，admin/driver 可以 drive。
- tmux 行直接打开底部抽屉，并标记 `shared`：所有 member 都能 attach；同一时刻只有一个浏览器 driver，其他人在抽屉里点 **Take over**。行内头像就是 Threadbase 内的 viewers，`N attached` 是 tmux 自己的 client 数（包含本机终端），`X driving` 是当前 driver。隐藏 presence 的账号不会出现在头像里。
- 如果 tmux 的 live cwd 位于某个已注册 project 内，行内显示 project 名，并出现 **+**：从该 session 创建一个 thread。服务端用 cwd 推断 project，线程默认使用隔离 worktree，首条消息可以预填“从 tmux session 继续”；同一 session 只能绑定一个 thread。
- 创建后 `tmux_links` 记录 `(socket, $N, created_at)` 指纹和 thread；会话被 kill 后 `$N` 被新 session 复用时不会误认领旧 thread。tmux 行会出现 `thread` 标记和打开入口。
- **深链**：`/sessions?session=tmux:<socket>:<$N>` 可以直接分享并在打开时 attach；关掉抽屉会移除 query，路径保持不变。
- 快速切换器：终端抽屉头部的列表按钮打开；桌面是居中 modal，手机是底部 sheet，支持搜索、方向键和 Enter。
- 这只是读模型，不新建 session 表：thread 仍然存在 SQLite，tmux 仍然是从 tmux 实时读取的运行时对象。`GET /api/sessions` 负责组合、排序和标记 partial failure（某个 tmux socket 读不到时返回 `warnings`，其余照常显示）。

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
- 终端字体：JetBrains Mono Nerd Font（OFL 1.1，`web/public/fonts/`）作为 xterm 的字体，保证 eza/lsd 等 Nerd Font 图标在没有本地字体的浏览器里也能显示；本机已安装时优先用 local 字体。
- 分发：单二进制，内嵌 web 资源；macOS 与 Linux 优先。



## 代码结构

```
crates/shared    wire 类型与领域模型（ts-rs 直接生成 web/src/api/types）
crates/core      纯领域策略：状态机（`state/` 一台机器一个文件）、投递决策、权限
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
| `threads` | 线程生命周期、worktree 与 DTO |
| `agents/` | `AgentManager` 门面、事件泵、队列 worker、生命周期副作用 |
| `approvals` / `elicitations` | 审批与提问的持久化、状态迁移、规则 |
| `queue` / `comments` / `subagents` / `bindings` | 各自实体的唯一写入点 |
| `inbox` / `notify` / `watchers` | inbox 记录、webhook、watcher 与通知 |
| `setup` / `search` / `push` | worktree 初始化钩子、FTS5 索引、Web Push 投递 |
| `events` | 追加式事件日志（每线程顺序、跨线程并行） |
| `diff` | 基线与审阅；线程列表不再触发任何 git 子进程 |
| `routes_*` | HTTP 路由，只做鉴权、解析和调用领域模块 |

前端入口按域拆分：`web/src/api/` 是 fetch 层，`components/thread/` 是时间线行组件，`components/composer/` 是 Composer 的命令与建议面板。线程、变更面板、设置与终端各自按需拆成 chunk；事件流按增量折叠并以 20fps 批量提交，长对话在流式输出时不会阻塞交互。

## 生命周期状态机

每个实体的合法状态、迁移和“当前能做什么”只有一处定义：`crates/core/src/state.rs`。服务端、前端和 agent 工具读同一个目录，不再各自比较状态字符串。

```bash
threadbase machines          # 人读的大纲
threadbase machines --json   # 与 GET /api/machines 完全相同
```

- 覆盖 14 台机器：thread、setup、approval、elicitation、subagent、queue、comment、watch、turn、auth_session、push、connection、draft、terminal。前三组是服务端领域状态，`turn`/`auth_session`/`push` 是进程内与投递状态，`connection`/`draft`/`terminal` 是浏览器侧生命周期（类型由 Rust 生成，前端不再手写字符串联合）。
- 每台机器区分**命令**与**事实**：命令是调用方可以主动执行的动作（`prompt`、`cancel`、`restore`），事实是适配器上报的事件（`turn_ended`、`process_exited`）。事实永远不会出现在按钮里。
- 每条命令带目标状态（`cancel -> idle`），所以 `threadbase machines` 输出的是完整的迁移图；目标值由同一组测试对照 `transition()` 校验，写错就测试失败。
- `ThreadDto.allowedActions` 与 `ThreadState.allowedActions` 由机器生成；前端通过 `web/src/lib/machine.ts` 的 `can()` / `threadCan()` 判断，所以按钮和服务端不会给出相反答案。
- `crates/core/tests/state_machines.rs` 会把目录导出到 `web/src/api/state-machines.json`。改了迁移就必须同步更新目录，否则测试失败。
- 新增一台机器：在 `crates/core/src/state/` 新建一个文件写 `XxxAction` 与 `XxxMachine`（`transition` + `commands`），在 `state/mod.rs` 声明并 `pub use`，在 `state/catalogue.rs` 的 `catalogue()` 加一条 `MachineSpec`，最后在 `web/src/lib/machine.ts` 的 `MachineId` 登记。一台机器一个文件，目录本身就是索引。

状态只保留一种表示：

- 归档同时写 `threads.status` 与 `archived_at`（单条 UPDATE，`archived_at` 不是独立开关）。
- 队列是否被取代只看 `queued_inputs.status`，`superseded` 布尔列已删除（迁移 `0019`）。
- 关注/静音是一个 `WatchState`，不是两个需要按顺序理解的布尔值。
- 推送投递用 `inbox.push_status` + `push_attempts` + `push_next_at`：失败会退避重试，不再把失败标成已送达（迁移 `0020`）。

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
pnpm --dir web doctor      # React Doctor 全量报告
pnpm --dir web doctor:ci   # CI 门：只有 correctness error 会失败
pnpm --dir web build
```

前端质量门是 `tsc --noEmit`、`vite build` 与 React Doctor（配置见 `web/doctor.config.ts`）。`web/audit/*.mjs` 是需要浏览器的手工审计脚本。

### 代码健康度：测量 → 候选 → 语义审查

```bash
pnpm --dir web quality          # 三层一起跑，报告写到 .quality/（已 gitignore）
scripts/quality/measure.sh dup      # jscpd：重复代码，阈值 1%（当前 0.16%）
scripts/quality/measure.sh dead     # knip：未使用的文件/导出/依赖
scripts/quality/measure.sh complex  # eslint：圈复杂度 >10、函数 >120 行
```

这三条不进门禁，只做诊断。第三层（读 `.quality/` 判断哪些候选该删）的规则写在 `scripts/quality/review.md`：生成物不删、`src/api/*` 的对称包装通常是产品缺口而非死代码、删除后必须过 `typecheck` + `doctor:ci` + `build`。

复杂度层用 eslint 但**不**把它加进 `web/package.json`：v2 前端刻意去掉了 eslint，而 `typescript@7` 没有 JS 编译 API。脚本把它装在 `~/.cache/threadbase-quality/`，依赖树不变。

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
