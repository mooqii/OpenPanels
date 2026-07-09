# OpenPanels Rust CLI Migration Spec

## 背景

OpenPanels 迁移前的 `openpanels-local` 由 TypeScript/Node.js 实现，运行时依赖
用户机器上的 Node.js。

当前 CLI 同时承担三类职责：

- 面向 agent 的命令协议：`agent context`、`panels`、`selection`、
  `wiki tasks next` 等。
- 本地 studio 进程管理：`studio start/status/open/wait/stop`。
- 内部本地 HTTP server：`__serve-studio` 启动 API 服务并托管
  `apps/local-studio/dist` 静态资源。

参考 `CreartCLI` 的方向，OpenPanels 下一阶段要把本地 CLI 和本地服务迁移为
Rust 原生二进制。最终用户安装后不再需要 Node.js、pnpm 或 npm 依赖树才能运行
`openpanels-local`。React/Vite studio 前端继续保留 TypeScript 实现，只在
release 构建时产出静态资源并嵌入 Rust binary。

## 目标

1. 交付一个 Rust 原生 `openpanels-local` binary，覆盖旧 CLI 的用户可见命令。
2. 用户运行 CLI 不需要本机安装 Node.js。
3. `studio start` 启动 Rust 内置 HTTP server，托管嵌入的 studio 静态资源。
4. 保持现有 `.myopenpanels/main.sqlite3` 数据格式和文件资产布局可读写。
5. 保持当前 agent-facing stdout/stderr、JSON 字段、exit code 语义稳定。
6. 保留 `apps/local-studio` 前端；canvas 作为 studio 内部模块维护。
7. 使用 GitHub Releases 发布多平台二进制。
8. 不再支持 npm 安装入口；安装以 GitHub Releases binary 和安装脚本为准。

## 非目标

- 不把 studio UI、canvas editor、React 组件迁移到 Rust。
- 不在本阶段改变 OpenPanels 产品模型、panel schema 或 wiki 业务逻辑。
- 不重写 canvas 渲染、Konva 交互、HeroUI 界面。
- 不引入云同步、多用户协作或远程数据库。
- 不在第一阶段强制完成代码签名、公证、Homebrew tap、winget、scoop 全渠道发布。
- studio UI 仍保留为 TypeScript/React；本地 CLI、server、storage 和 control 逻辑已迁移到 Rust。

## 关键决策

- Rust 负责本地 native surface：CLI、HTTP server、SQLite storage、agent context
  渲染、studio 进程管理。
- TypeScript 继续负责 browser surface：studio app、canvas UI、前端 domain 交互。
- CLI 命令协议以迁移前的 TypeScript CLI 为迁移基准，不借迁移扩大命令集；当前
  实现权威已经转到 `crates/openpanels-local/src/cli.rs`。
- 前端数据类型保留在 `apps/local-studio/src/protocol.ts`。
- SQLite schema 以 `crates/openpanels-local/src/storage.rs` 为权威实现。
- `agent-capabilities` 应迁移为语言无关 manifest，避免 Rust 和 TypeScript 重复维护。
- Release binary 内嵌 `apps/local-studio/dist`，同时保留
  `OPENPANELS_STUDIO_STATIC_DIR` 作为开发和调试覆盖入口。

## 当前实现盘点

### 已移除的 Node CLI

迁移前 CLI 包含：

```text
TypeScript CLI command implementation
Node.js executable entry
agent context/capabilities/guides rendering
```

当前不再保留 Node CLI package，也不再通过 npm 发布安装入口。

### 当前命令面

Rust CLI 必须覆盖这些命令和兼容 alias：

```text
openpanels-local version
openpanels-local --version
openpanels-local help

openpanels-local studio start
openpanels-local studio status
openpanels-local studio open
openpanels-local studio wait
openpanels-local studio stop

openpanels-local agent context
openpanels-local agent capabilities
openpanels-local agent guides
openpanels-local agent guide <id>
openpanels-local agent-context

openpanels-local panels
openpanels-local active-panel
openpanels-local panel-state
openpanels-local canvas-state
openpanels-local selection
openpanels-local read-selection-asset
openpanels-local insert-placeholder
openpanels-local insert-image

openpanels-local wiki context
openpanels-local wiki agent-target register
openpanels-local wiki agent-target list
openpanels-local wiki raw add
openpanels-local wiki raw new-markdown
openpanels-local wiki raw add-text
openpanels-local wiki raw list
openpanels-local wiki markdown read
openpanels-local wiki markdown write
openpanels-local wiki tasks list
openpanels-local wiki tasks next
openpanels-local wiki tasks claim
openpanels-local wiki tasks complete
openpanels-local wiki tasks fail
openpanels-local wiki spaces list
openpanels-local wiki spaces active
openpanels-local wiki pages list
openpanels-local wiki pages read
openpanels-local wiki pages create
openpanels-local wiki pages write

openpanels-local __serve-studio
```

通用参数：

```text
--project <dir>
--storage-dir <dir>
--context-id <id>
--host <host>
--local-only
--format json
--version
--help
```

### 当前本地 HTTP API

Rust server 必须覆盖现有 studio API：

```text
GET  /api/bootstrap
POST /api/projects
GET  /api/sessions
GET  /api/active-session
PUT  /api/active-session
GET  /api/active-panel
PUT  /api/active-panel
PATCH /api/sessions/{sessionId}
DELETE /api/sessions/{sessionId}
POST /api/sessions
POST /api/panels
POST /api/artifacts
PUT  /api/panels/{sessionId}/{panelId}/state
PUT  /api/panels/{sessionId}/{panelId}/selection
POST /api/panels/{sessionId}/{panelId}/assets
GET  /api/panels/{sessionId}/{panelId}/assets/{assetRef}

GET  /api/wiki/context
GET  /api/wiki/raw-documents
POST /api/wiki/raw-documents
GET  /api/wiki/raw-documents/{documentId}/markdown
PUT  /api/wiki/raw-documents/{documentId}/markdown
GET  /api/wiki/raw-documents/{documentId}/original
POST /api/wiki/raw-documents/{documentId}/reveal
POST /api/wiki/raw-documents/{documentId}/extract
POST /api/wiki/raw-documents/{documentId}/reindex
DELETE /api/wiki/raw-documents/{documentId}
GET  /api/wiki/tasks
GET  /api/wiki/tasks/next
POST /api/wiki/tasks/{taskId}/claim
POST /api/wiki/tasks/{taskId}/complete
POST /api/wiki/tasks/{taskId}/fail
GET  /api/wiki/agent-targets
POST /api/wiki/agent-targets
GET  /api/wiki/active-space
PUT  /api/wiki/active-space
GET  /api/wiki/language
PUT  /api/wiki/language
POST /api/wiki/language
GET  /api/wiki/spaces
POST /api/wiki/spaces/{wikiSpaceId}/reindex
GET  /api/wiki/spaces/{wikiSpaceId}/pages
GET  /api/wiki/spaces/{wikiSpaceId}/pages/{pagePath}
POST /api/wiki/spaces/{wikiSpaceId}/pages
PUT  /api/wiki/spaces/{wikiSpaceId}/pages/{pagePath}
```

静态资源：

- `/` 返回 `index.html`。
- `/assets/...` 返回前端构建产物。
- SPA fallback 返回 `index.html`。

## 目标架构

### 目录结构

在 `crates/openpanels-local/` 引入 Rust crate，保持仓库根目录继续作为 pnpm
monorepo 和产品文档入口：

```text
OpenPanels/
  Cargo.toml                 # workspace, optional but recommended
  Cargo.lock
  crates/
    openpanels-local/
      Cargo.toml
      build.rs
      src/
        main.rs
        lib.rs
        cli.rs
        error.rs
        types.rs
        paths.rs
        storage.rs
        migrations.rs
        control.rs
        server.rs
        studio.rs
        agent.rs
        assets.rs
        wiki.rs
        canvas.rs
        process.rs
  apps/local-studio/
  agent-guides/
  docs/
```

binary 名称必须是 `openpanels-local`。根目录 `Cargo.toml` 只作为 Rust workspace
入口，避免把 CLI crate、pnpm workspace、前端包的职责混在同一层。

### Rust 模块职责

- `main.rs`：收集 argv，调用 `openpanels_local::run_cli`，按返回码退出。
- `cli.rs`：命令解析、格式化输出、exit code、alias、参数校验。
- `types.rs`：OpenPanels session、panel、artifact、wiki、selection、CLI JSON 输出类型。
- `paths.rs`：解析 project/storage/context 路径和环境变量。
- `storage.rs`：SQLite-backed storage，实现当前 `LocalOpenPanelsStorage` 行为。
- `migrations.rs`：Rust 版本 SQLite migrations 和 checksum/版本记录。
- `control.rs`：迁移当前 `@openpanelsRust control/wiki modules` 的 project/panel/bootstrap 业务逻辑。
- `wiki.rs`：wiki raw document、task、space、page 操作。
- `canvas.rs`：canvas selection、asset、insert image、insert placeholder 操作。
- `server.rs`：Axum HTTP server 和 route。
- `studio.rs`：studio start/status/wait/stop/open、静态资源定位。
- `agent.rs`：agent capabilities、agent context markdown/json、guide 列表和读取。
- `assets.rs`：内嵌 studio dist、agent guides、MIME/type、文件读取。
- `process.rs`：pid 检查、后台进程启动、终止、跨平台打开浏览器。
- `error.rs`：用户可见错误、JSON 错误、exit code。

### 推荐依赖

```toml
[dependencies]
axum = "0.8"
base64 = "0.22"
clap = { version = "4.5", features = ["derive"] }
include_dir = "0.7"
mime_guess = "2"
open = "5"
rand = "0.9"
rusqlite = { version = "0.32", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
thiserror = "2"
time = { version = "0.3", features = ["formatting", "local-offset", "macros"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread", "signal", "net"] }
tower-http = { version = "0.6", features = ["cors", "fs"] }
urlencoding = "2"
walkdir = "2"
```

说明：

- `rusqlite` 更适合当前同步 SQLite 使用方式；如果未来需要 async pool，可再切换
  `sqlx`。
- `include_dir` 用于嵌入 `apps/local-studio/dist`、`agent-guides` 和 capabilities
  manifest。
- `open` 可以替代手写 `open/cmd/xdg-open`。

## 构建与静态资源

### build.rs

Release 构建应先构建 studio：

```text
cargo build --release
  -> build.rs
    -> pnpm --filter @openpanels/local-studio build
    -> 校验 apps/local-studio/dist/index.html 存在
    -> Rust 编译 include_dir 嵌入 dist
```

开发构建不强制自动 build 前端，避免 `cargo check` 过慢。开发时按优先级查找静态资源：

1. `OPENPANELS_STUDIO_STATIC_DIR`
2. `apps/local-studio/dist`
3. 内嵌 `include_dir`

Release binary 必须不依赖外部 `dist/` 目录。

### 前端仍然保留 pnpm 工作区

保留：

```text
apps/local-studio
```

其中 canvas 已内联到 `apps/local-studio/src/canvas`，前端协议类型位于
`apps/local-studio/src/protocol.ts`。Rust 侧维护等价 `serde` 类型。

## CLI 行为规范

### 输出格式

- 默认 `text` 输出必须尽量保持现有文本。
- `--format json` 输出必须保持当前字段名。
- 出错时：
  - text mode 写 stderr：`Error: <message>\n`
  - json mode 写 stdout：
    ```json
    {
      "ok": false,
      "error": "<message>"
    }
    ```
  - exit code 为 `1`。
- 参数值非法、缺少必填参数也先保持 exit code `1`，除非后续明确升级为 `2`。

### 版本

Rust binary 版本来自 `CARGO_PKG_VERSION`，并和 root `package.json` 版本保持一致。

```text
openpanels-local version
openpanels-local --version
```

均输出同一版本。

### studio start

当前行为：

1. 根据 `--project`、`--storage-dir`、`--context-id` 解析路径。
2. 默认 host 为 `0.0.0.0`，`--local-only` 时使用 `127.0.0.1`。
3. 若已有同 host 健康 session，复用。
4. 若旧 pid 还在但不健康，终止旧进程。
5. 随机分配可用 port。
6. 后台启动同一 CLI 的 `__serve-studio`。
7. 写入 `studio-session.json`。
8. 等待 `/api/bootstrap` 健康。
9. 输出 session JSON 或 server URL。

Rust 必须保持同样行为。后台进程启动改为：

```text
std::env::current_exe()
  __serve-studio
  --project ...
  --storage-dir ...
  --context-id ...
  --port ...
  --host ...
```

Release 模式下不需要传 `--static-dir`，因为资源已内嵌。开发模式可以继续接受
`--static-dir` 作为隐藏参数。

### studio session 文件

继续写入：

```text
<contextDir>/studio-session.json
```

结构保持：

```json
{
  "browserUrl": "http://192.168.x.x:12345",
  "contextDir": "...",
  "contextId": "...",
  "contextIdSource": "...",
  "host": "0.0.0.0",
  "lanServerUrls": ["..."],
  "localServerUrl": "http://127.0.0.1:12345",
  "logPath": ".../studio.log",
  "pid": 12345,
  "port": 12345,
  "projectDir": "...",
  "serverUrl": "http://127.0.0.1:12345",
  "startedAt": "...",
  "storageDir": "..."
}
```

### 路径解析

Rust 需要迁移 `resolveOpenPanelsPaths`：

- `projectDir`：`--project` > `OPENPANELS_PROJECT_DIR` > cwd。
- `storageDir`：`--storage-dir` > `OPENPANELS_STORAGE_DIR` > `<projectDir>/.myopenpanels`。
- `contextId`：`--context-id` > agent/thread 环境变量 > cwd-derived default。
- `contextDir`：`<storageDir>/contexts/<contextId>`。

上下文 ID 生成规则必须通过 golden tests 锁住，因为它影响 studio session 隔离。

## Storage 迁移规范

### SQLite 文件

继续使用：

```text
<storageDir>/main.sqlite3
```

保留同一组表：

```text
sessions
panels
panel_states
artifacts
panel_selections
wiki_tasks
key_values
```

大文件仍存文件系统，例如：

```text
<storageDir>/sessions/<sessionId>/panels/<panelId>/assets/
<storageDir>/sessions/<sessionId>/panels/<panelId>/raw/
<storageDir>/sessions/<sessionId>/panels/<panelId>/wikis/
```

### Schema 兼容

Rust migration `0001_initial` 必须和 TypeScript migration 等价。迁移完成后：

- Rust 能读取 TS 创建的数据库。
- TS studio 在迁移期仍能读取 Rust 写入的数据。
- JSON 列字段名保持 camelCase。
- 时间戳继续使用 ISO 8601 字符串。

### 事务

这些操作必须使用事务：

- `deleteSession`
- `writePanelState` 加 wiki task index sync
- wiki raw document add/delete/extract/reindex
- wiki task claim/complete/fail
- page write + task creation
- selection state + asset write 的最终提交

### 文件安全

Rust 需保留当前安全边界：

- storage root 必须可创建且不能是危险根目录。
- 解码 URL 路径后必须防止 `..` 越界。
- asset/raw/page 文件名要 sanitize。
- 写文件先确保父目录存在。
- 对用户传入 output path 只写明确指定路径。

## Agent Context 与 Guides

当前 agent 入口依赖：

```text
openpanels-local agent context --project "$PWD"
```

迁移后它仍是协议权威。

### Capabilities manifest

迁移前计划把 capability manifest 迁移为语言无关文件：

```text
agent-guides/capabilities.json
```

当前第一版直接由 `crates/openpanels-local/src/agent.rs` 维护 capability 和 guide
渲染；如果前端后续也需要消费同一份能力列表，再拆成共享 JSON。

### Guides

继续使用顶层：

```text
agent-guides/*.md
```

Rust binary release 时嵌入这些 markdown。`agent guide <id>` 必须支持当前
task context 注入逻辑，尤其是 wiki task guide 里的命令模板。

## 本地 HTTP Server

Rust server 使用 Axum 实现：

```text
openpanels-local __serve-studio
  --project <dir>
  --storage-dir <dir>
  --context-id <id>
  --port <port>
  --host <host>
```

要求：

- 默认只由 `studio start` 内部启动。
- 支持 `SIGTERM`、`SIGINT` 优雅退出。
- 写 stdout/stderr 到 `studio.log`。
- CORS 行为保持当前实现：允许 studio 前端调用本地 API。
- 健康检查仍使用 `GET /api/bootstrap`。
- API error 返回 JSON `{ "error": "<message>" }`，status 500。
- 未命中 API 时尝试静态资源；SPA fallback 返回 `index.html`。

## Canvas 操作迁移

Rust 需要提供 canvas CLI 操作：

- `getCanvasState`
- `getSelection`
- `readSelectionAsset`
- `writePanelAsset`
- `insertImage`
- `insertPlaceholder`
- `emptyCanvasSnapshot`
- `normalizeSerializableSnapshot`
- `dataUrlToBuffer`
- `mimeTypeForFile`

行为要求：

- `selection --include-image-base64` 可返回 selected shapes 和 PNG base64。
- `read-selection-asset --output <path>` 写出 selection asset。
- `insert-image` 支持 `--placement right|left|below`、`--anchor-shape-id`、
  `--replace-shape-id`、`--display-width`、`--display-height`、`--file-name`。
- `insert-placeholder` 支持 `--display-width`、`--display-height`、
  `--anchor-shape-id`、`--text`。
- shape ID、asset ref、panel state JSON 结构不能变化。

注意：Rust 不负责 canvas 渲染，只修改持久化 JSON state。studio 前端继续解释并渲染
这些 state。

## Wiki 操作迁移

Rust 需要提供 wiki control 行为：

- raw document add/new markdown/add text/list
- markdown read/write
- task list/next/claim/complete/fail
- agent target register/list
- space list/active
- page list/read/write/create
- language get/set API
- raw original read/reveal/extract/reindex/delete API

要求：

- task 状态机保持：`queued`、`claimed`、`running`、`failed`、`succeeded`、`stale`。
- task claim 时绑定 process/thread/wiki space 的逻辑保持。
- task complete/fail 后同步 raw document conversion 状态和 wiki task index。
- page write 后更新时间、page index、task 关系。
- raw document original 文件继续保存在 storage 文件树，不写入 SQLite BLOB。

## 发布与安装

### 支持平台

第一阶段建议支持：

```text
macOS arm64
macOS x64
Linux x64
Linux arm64
Windows x64
```

Release asset 命名建议：

```text
openpanels-local-aarch64-apple-darwin.tar.gz
openpanels-local-x86_64-apple-darwin.tar.gz
openpanels-local-x86_64-unknown-linux-gnu.tar.gz
openpanels-local-aarch64-unknown-linux-gnu.tar.gz
openpanels-local-x86_64-pc-windows-msvc.zip
```

发布约束、manifest schema、自更新缓存策略以 `docs/release.md` 为准。release tag、
Rust crate version 和 root package version 必须保持一致；CLI 自更新只读取
GitHub Releases，不依赖 OpenPanels 云服务。

### 自更新

Rust CLI 提供：

```text
openpanels-local update check
openpanels-local update download
openpanels-local update
```

`update check` 读取 GitHub Releases latest manifest 并缓存结果。普通 text-mode 命令
最多每 24 小时做一次 opportunistic check；发现新版本时只向 stderr 输出简短提示。
`--format json` 不做 opportunistic check，避免污染稳定 JSON 输出。`update download`
下载并缓存当前平台 release asset，校验 SHA-256，但不安装。`update` 优先复用已缓存
asset，运行新 binary 的 `--version` 验证版本，然后替换当前 executable。studio UI
可以在右下角显示更新入口；预下载可以后台完成，但安装和重启必须由用户点击或明确
授权 agent 后触发。开发构建和 Homebrew 等包管理器路径默认拒绝自替换。

### 安装脚本

参考 `CreartCLI`：

```text
scripts/install-openpanels-local.sh
scripts/install-openpanels-local.ps1
```

脚本职责：

1. 检测 OS/arch。
2. 下载匹配 release asset。
3. 校验 checksum。
4. 解压为 `openpanels-local` 或 `openpanels-local.exe`。
5. 安装到用户 PATH 中的目录，例如 `~/.local/bin`。
6. 输出 `openpanels-local --version` 验证结果。

### 签名策略

第一版发布未签名 binary，不处理 macOS notarization 或 Windows Authenticode 签名。
需要在 README 和安装脚本输出里记录风险：

- macOS 可能出现 Gatekeeper/quarantine 提示。
- Windows 可能出现 SmartScreen 提示。

后续面向更广泛非开发者分发前再补：

- macOS codesign + notarization。
- Windows Authenticode 签名。
- Release checksums 和 provenance。

## 测试计划

### Golden CLI tests

在迁移前先固化当前 Node CLI 输出：

```text
tests/golden/
  help.txt
  version.txt
  studio-status-missing.json
  panels-empty.json
  agent-context.md
  wiki-tasks-empty.json
```

Rust 迁移后逐项对比。重点锁定：

- stdout/stderr 位置。
- JSON 字段名和嵌套结构。
- text mode 文案。
- exit code。

### Storage compatibility tests

测试矩阵：

1. TS 写入数据库，Rust 读取。
2. Rust 写入数据库，TS studio 读取。
3. Rust 写入 panel state，studio 前端打开正常。
4. Rust 写入 wiki task，CLI 和 studio 都能列出。
5. Rust 写入 selection asset，`read-selection-asset` 能导出。

### HTTP API tests

使用 `reqwest` 或 `axum-test` 覆盖：

- `/api/bootstrap`
- active session/panel set/get
- session create/rename/delete
- panel state save
- selection save
- asset upload/read
- wiki raw document lifecycle
- wiki task lifecycle
- static resource fallback

### End-to-end tests

CI 中运行：

```bash
cargo test
pnpm -r typecheck
pnpm --filter @openpanels/local-studio build
```

可选：

```bash
openpanels-local studio start --project <tmp> --format json
openpanels-local agent context --project <tmp>
openpanels-local insert-placeholder --project <tmp> --format json
openpanels-local studio stop --project <tmp> --format json
```

## 分阶段实施

### Phase 0: Contract Freeze

- 为当前 Node CLI 增加/整理 golden tests。
- 导出 capabilities 到 JSON manifest。
- 明确 `resolveOpenPanelsPaths`、context ID、session JSON 的稳定规则。
- 为 SQLite schema 加 Rust 可复用 schema 文档。

### Phase 1: Rust Skeleton

- 新增根目录 Rust workspace 和 `crates/openpanels-local/Cargo.toml`。
- 新增 `crates/openpanels-local/src/main.rs`、`src/lib.rs`。
- 实现 `version/help`、参数解析、错误输出、`--format json`。
- 实现 static asset embedding 的 build path。
- CI 加 `cargo test`。

### Phase 2: Storage + Control

- 迁移 SQLite migrations 和 storage CRUD。
- 迁移 project bootstrap、session/panel/artifact 操作。
- 迁移 active session/panel 和 context paths。
- 完成 `panels`、`active-panel`、`panel-state`、`canvas-state`。

### Phase 3: Studio Server

- 用 Axum 实现 `__serve-studio`。
- 实现 `studio start/status/wait/open/stop`。
- 嵌入并托管 studio dist。
- 确认浏览器打开现有 studio 可正常读写项目。

### Phase 4: Agent + Canvas + Wiki

- 迁移 agent context/capabilities/guides。
- 迁移 selection、read-selection-asset、insert-placeholder、insert-image。
- 迁移 wiki CLI 和 wiki HTTP API。
- 对齐 agent guides 中所有命令示例。

### Phase 5: Release

- GitHub Actions 多平台构建。
- 上传 release assets 和 checksums 到 GitHub Releases。
- 增加 `scripts/install-openpanels-local.sh` 和
  `scripts/install-openpanels-local.ps1`。
- 更新 README、docs、skill。
- 删除 npm 安装入口，安装脚本直接下载 GitHub Releases native binary。

## 验收标准

Rust 迁移完成时必须满足：

1. 干净机器安装 native binary 后，不安装 Node.js 也能运行
   `openpanels-local --version`、`agent context`、`studio start`。
2. `studio start` 返回的 `browserUrl` 能打开现有 studio UI。
3. studio UI 能创建/切换 project、panel，保存 canvas 和 wiki state。
4. 现有 agent skill 中核心命令继续可用。
5. `--format json` 输出和旧 CLI 兼容。
6. 当前 `.myopenpanels` SQLite 数据能被 Rust CLI 读取。
7. Release asset 在 macOS/Linux/Windows 对应平台可执行。
8. CI 同时覆盖 Rust tests 和前端 TypeScript checks。

## 已确认决策

- Rust crate 放在 `crates/openpanels-local/`。
- npm 安装入口不保留；第一版只通过 GitHub Releases binary 和安装脚本分发。
- Release assets 放 GitHub Releases。
- 第一版不处理 macOS notarization 或 Windows 签名。
- Rust 迁移时不顺手清理文档里提到但当前未实现的 wiki 命令，先保持当前代码行为和迁移边界。
- 第一版只提供 `openpanels-local` 命令，不增加 `op`、`opl` 等短 alias。

## 待确认问题

当前没有阻塞第一版 Rust 迁移实现的未确认产品/发布决策。
