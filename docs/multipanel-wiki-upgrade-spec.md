# MyOpenPanels Multi-Panel Wiki Upgrade Spec

> Historical upgrade document. Its `agent context` examples are superseded by
> Protocol v3 `agent bootstrap`; see `agent-guidance-protocol-spec.md`.
> Its command examples are superseded by the 0.4 vNext command tree.

## 背景

MyOpenPanels 当前已经有一套接近多面板的底层模型：

- `Session` 实际上就是用户看到的 Project。
- `Session.panelIds` 已经允许一个 Project 持有多个 panel。
- 每个 panel 已经独立持久化 `panel.json` 和 `state.json`。
- Rust control 层已经可以按 `kind` 创建和读取不同 panel。

当前限制主要在产品入口和命名上：本地 studio、CLI、agent skill 仍然以 canvas 为中心。增加 wiki 面板时，应该顺手把产品模型升级为：

```text
Project(Session)
  - title
  - panels
    - wiki: 文档库
    - canvas: Canvas
```

目标不是一次性实现完整 wiki 编辑器，而是先搭好多面板架子，让后续 wiki、canvas 和更多 panel 能共享同一个 Project、同一个 studio、同一个 CLI 能力发现机制。

## 目标

1. 在一个 Project 内同时存在 `wiki` 和 `canvas` 两个面板。
2. 用户通过 studio 底部 HeroUI 3 Tabs 在 `文档库` 和 `Canvas` 之间切换。
3. 切换面板不改变当前 Project 名称、不改变当前 active session。
4. 初期 wiki 面板只提供空状态和占位 UI，不做完整编辑器功能。
5. 建立面向未来的多面板 API、CLI 和 agent context 架构。
6. 将 MyOpenPanels agent skill 简化为稳定入口：只负责安装/调用最新 CLI，具体面板说明由 CLI 返回。

## 非目标

- 本阶段不实现完整 markdown 编辑器。
- 本阶段不实现 wiki 搜索、目录树、链接关系、版本历史。
- 本阶段不强制迁移旧 canvas 数据格式。
- 本阶段不移除现有 canvas 一级 CLI 命令，先保持兼容。

## 产品模型

### 命名

- 用户界面里称为 Project。
- 协议和存储层可以暂时继续使用 `MyOpenPanelsSession`，避免大规模重命名。
- 文档和新增代码里尽量使用 Project 语义包装 Session，例如 `ProjectBootstrap`。

### Panel kind

新增：

```ts
type MyOpenPanelsPanelKind =
  | "wiki"
  | "canvas"
  | "image"
  | "diff"
  | "preview"
  | "files"
```

首屏 tabs 顺序建议为：

1. `wiki`，中文显示 `文档库`
2. `canvas`，显示 `Canvas`

这样强调 Project 的结构化知识沉淀，而 canvas 作为视觉工作区存在。

## Wiki 初始状态

本阶段 wiki 可以只有最小可持久化状态：

```ts
export interface WikiState {
  schemaVersion: 1
  pages: WikiPage[]
  activePageId: string | null
}

export interface WikiPage {
  id: string
  title: string
  path: string
  markdown: string
  createdAt: string
  updatedAt: string
}
```

初始状态：

```json
{
  "schemaVersion": 1,
  "pages": [],
  "activePageId": null
}
```

备注：

- `path` 用于未来构建层级目录，例如 `README`, `research/notes`, `design/system`。
- `markdown` 暂时存在 panel state 里，后续如果内容变大，可以迁移到 panel assets 或 project files。
- `schemaVersion` 从一开始保留，方便以后升级 wiki 数据结构。

## 存储结构

沿用当前结构：

```text
.myopenpanels/
  sessions/
    <sessionId>/
      session.json
      panels/
        <wikiPanelId>/
          panel.json
          state.json
        <canvasPanelId>/
          panel.json
          state.json
          selection.json
          assets/
```

旧 Project 打开时：

1. 如果没有 `canvas` panel，创建一个。
2. 如果没有 `wiki` panel，创建一个。
3. 保留旧 Project title 和已有 canvas panel id。

## Runtime 和 Core 改造

### protocol

文件：`apps/studio/src/protocol.ts`

- `panelKindSchema` 增加 `"wiki"`。
- 如需要，可导出 `wikiStateSchema`，但当前状态约定主要由 Rust control/wiki 和 studio 前端共同维护。

### core

文件：`crates/myopenpanels/src/control.rs`

- `createDefaultPanelRegistry()` 注册 wiki panel。
- `defaultTitleForPanel("wiki")` 返回 `文档库` 或 `Wiki`。

建议：

```ts
registry.register({
  kind: "wiki",
  title: "文档库",
  canHandleArtifact: (artifact) => artifact.kind === "file",
  createInitialState: () => ({
    schemaVersion: 1,
    pages: [],
    activePageId: null,
  }),
})
```

`canHandleArtifact` 本阶段可以先保守返回 `false`，避免文件 artifact 被错误塞进 wiki。等 wiki import markdown 时再打开。

## Rust Control 改造

文件：`crates/myopenpanels/src/control.rs`

当前核心函数是 `ensureCanvasBootstrap()`。建议新增通用函数，旧函数保留为兼容包装。

### 新增类型

```ts
export interface ProjectPanelSnapshot {
  panel: MyOpenPanelsPanel
  state: unknown
}

export interface ProjectBootstrap {
  activePanelId: string
  activePanelKind: MyOpenPanelsPanelKind
  contextDir: string
  contextId: string
  contextIdSource: string
  panels: ProjectPanelSnapshot[]
  session: MyOpenPanelsSession
  sessions: MyOpenPanelsSession[]
  storageDir: string
}
```

### 新增 helper

```ts
ensureProjectBootstrap(context, {
  requestedSessionId,
  requestedPanelKind,
})
```

职责：

- 解析 active session。
- 没有 session 时创建 Project。
- 确保 Project 至少有 `wiki` 和 `canvas`。
- 返回 Project 下所有已注册/已创建面板及其 state。
- 写入 active session。
- 读写 active panel。

### active panel 持久化

新增：

```text
<contextDir>/active-panel.json
```

结构：

```json
{
  "sessionId": "session:...",
  "panelId": "panel:...",
  "kind": "wiki",
  "updatedAt": "..."
}
```

规则：

- active panel 是 conversation-local 状态，和 active session 一样放在 `contextDir`。
- 切 Project 后，如果该 Project 有同 kind panel，优先保持 kind；否则默认 `wiki`。
- canvas 专属 CLI 命令仍然可以直接 ensure canvas，不受 active panel 影响。

## Rust Server API

文件：`crates/myopenpanels/src/server.rs`

### 修改 bootstrap

现有：

```http
GET /api/bootstrap
```

从返回 canvas bootstrap 改为返回 project bootstrap。

建议返回：

```json
{
  "session": {},
  "sessions": [],
  "panels": [
    { "panel": { "kind": "wiki" }, "state": {} },
    { "panel": { "kind": "canvas" }, "state": {} }
  ],
  "activePanelId": "panel:...",
  "activePanelKind": "wiki"
}
```

为降低前端改造风险，第一阶段也可以同时保留旧字段：

```json
{
  "panel": "<active panel>",
  "state": "<active panel state>"
}
```

但 studio 内部应该尽快使用 `panels`。

### 新增 active panel API

```http
GET /api/active-panel
PUT /api/active-panel
```

`PUT` body：

```json
{
  "sessionId": "session:...",
  "panelId": "panel:..."
}
```

或：

```json
{
  "sessionId": "session:...",
  "kind": "wiki"
}
```

### 继续保留 state API

```http
PUT /api/panels/:sessionId/:panelId/state
```

Wiki 和 canvas 都使用这个通用 state 保存入口。

## Studio UI

文件：`apps/studio/src/main.tsx`

### 新组件建议

```text
App
  ProjectShell
    ProjectTitleControl
    ActivePanelHost
    BottomPanelTabs
```

### ProjectShell 职责

- 持有 `ProjectBootstrap`。
- 持有 `activePanelId`。
- 根据 active panel kind 渲染不同 panel。
- 切换 tabs 时调用 `PUT /api/active-panel`，同时本地切换 state。
- Project title 控件保持现有行为。

### Canvas 渲染

现有 `CanvasPanel` 基本保持不变：

- 只在 active panel kind 是 `canvas` 时渲染。
- `assetStore` 使用 canvas panel id。
- `selection` 保存只对 canvas panel 启用。
- `snapshotVersion` 仍用于远程变化刷新。

### Wiki 占位面板

新增组件可先放在 `apps/studio/src/main.tsx`，后续再拆包：

```tsx
function WikiPanelPlaceholder({ state }: { state: WikiState }) {
  return (
    <section className="op-wiki-panel">
      <div className="op-wiki-panel__empty">
        <h1>文档库</h1>
        <p>这里将用于和 agent 协作整理结构化 markdown wiki。</p>
      </div>
    </section>
  )
}
```

注意：这个文案是空状态，不是功能说明。后续正式 wiki UI 可以删除。

### 底部 Tabs

使用 HeroUI 3 Tabs。视觉位置建议：

- 固定在底部中间。
- 与 canvas toolbar 避免重叠。
- z-index 低于项目菜单和确认弹窗，高于 panel 内容。

状态：

- `selectedKey` = active panel kind。
- tab key 使用 panel kind，而不是 panel id。
- 当前 Project 内同 kind 只允许一个 panel。

## CLI 架构

文件：`crates/myopenpanels/src/cli.rs`

### 稳定入口原则

入口 skill 不再承载具体 panel 操作说明。CLI 是能力说明和面板协议的来源。

入口 skill 固定做：

```bash
myopenpanels agent context --project-dir "$PWD"
```

然后遵循 CLI 返回的最新说明。

### 当前 canonical 命令

CLI 不保留旧的扁平命令或隐式子命令。面板发现与读取使用：

```text
myopenpanels agent context
myopenpanels panel list
myopenpanels panel current
myopenpanels panel switch
myopenpanels wiki context
myopenpanels canvas state
```

#### `agent context`

返回给 agent 的最新工作说明。

JSON 结构建议：

```json
{
  "cliVersion": "0.1.x",
  "project": {
    "id": "session:...",
    "title": "..."
  },
  "activePanel": {
    "id": "panel:...",
    "kind": "wiki",
    "title": "文档库"
  },
  "panels": [
    { "id": "panel:...", "kind": "wiki", "title": "文档库" },
    { "id": "panel:...", "kind": "canvas", "title": "Canvas" }
  ],
  "commands": {
    "studioStart": "myopenpanels studio start --project-dir \"$PWD\" --format json",
    "agentContext": "myopenpanels agent context --project-dir \"$PWD\""
  },
  "panelInstructions": {
    "wiki": "...",
    "canvas": "..."
  }
}
```

Markdown 输出面向 agent 直接阅读，内容应包括：

- 当前 Project 和 active panel。
- 可用 panels。
- 如何切换/读取 panel。
- canvas 相关命令。
- wiki 当前阶段说明：可以识别 wiki 面板，但暂不支持写入具体页面。

#### `panel list`

列出当前 Project 的 panels：

```bash
myopenpanels panel list --project-dir "$PWD" --format json
```

#### `panel current` / `panel switch`

读取或切换 active panel：

```bash
myopenpanels panel current --project-dir "$PWD" --format json
myopenpanels panel switch --project-dir "$PWD" --kind wiki --format json
myopenpanels panel switch --project-dir "$PWD" --kind canvas --format json
```

#### Panel state

使用 panel 对应的 canonical context/state 命令：

```bash
myopenpanels wiki context --project-dir "$PWD" --format json
myopenpanels canvas state --project-dir "$PWD" --format json
```

Canvas 只使用 namespace 命令：

```text
canvas state
canvas selection read
canvas selection export
canvas placeholder create
canvas image insert
```

## Agent Skill 策略

新增或替换为一个稳定入口 skill：`MyOpenPanels`。

入口 skill 只负责：

1. 确保全局 `myopenpanels` native binary 已安装且最新。
2. 启动 studio。
3. 调用 `agent context`。
4. 严格遵循 `agent context` 返回的最新 panel 说明。

不建议让用户分别安装：

- `MyOpenPanels-canvas`
- `MyOpenPanels-wiki`

这些可以作为 CLI 内部 manual，不作为用户级 skill 分发。这样即使用户长期不更新入口 skill，只要它仍然会调用 latest CLI，就能获取最新 panel 能力。

### 入口 skill 草案

```md
Use this skill when the user wants to use MyOpenPanels.

Set:
MYOPENPANELS_CLI="${MYOPENPANELS_CLI:-myopenpanels}"

Start or reuse the studio:
$MYOPENPANELS_CLI studio start --project-dir "$PWD" --format json

Before interacting with any panel, run:
$MYOPENPANELS_CLI agent context --project-dir "$PWD"

Follow the returned MyOpenPanels instructions. The CLI is the source of truth
for available panels, commands, and panel-specific workflows.
```

## Implementation Phases

### Phase 1: Multi-panel skeleton

Scope:

- Add `wiki` panel kind.
- Register wiki in default panel registry.
- Add `ensureProjectBootstrap`.
- Ensure every Project has wiki and canvas panels.
- Update studio bootstrap to return all panels.
- Add bottom HeroUI Tabs.
- Render wiki placeholder.
- Expose canonical namespace-based canvas commands.

Acceptance criteria:

- New Project opens with `文档库` and `Canvas` tabs.
- Switching tabs does not change Project title.
- Canvas content persists after switching to wiki and back.
- Old Project with only canvas gets a wiki panel automatically.
- `canvas selection read` and `canvas image insert` operate on canvas.

### Phase 2: CLI capability discovery

Scope:

- Add `agent context`.
- Add `panel list/current/switch`.
- Add `wiki context` and `canvas state`.
- Update docs for stable MyOpenPanels entry skill.

Acceptance criteria:

- `agent context` gives enough instructions for agent to understand wiki/canvas.
- `panel switch --kind wiki` switches the studio active panel state.
- `wiki context` returns Wiki state and agent guidance.
- User does not need `MyOpenPanels-wiki` skill to discover wiki panel.

### Phase 3: Wiki write API

Scope:

- Add `wiki pages list/search/read/write`.
- Add `wiki documents list/add/create-markdown`.
- Add `wiki markdown read/write`.

Acceptance criteria:

- Agent can create/update structured markdown pages.
- Studio wiki placeholder can display page list or active markdown preview.
- Wiki state remains schema-versioned.

## Testing Plan

### Unit tests

- `protocol`: `panelKindSchema` accepts `wiki`.
- `core`: registry returns wiki definition and default title.
- Rust control: `ensure_project_bootstrap` creates/loads both wiki and canvas.
- Rust control: canvas bootstrap commands still return the canvas panel.
- Rust CLI: new commands return stable JSON.

### Server tests

- `GET /api/bootstrap` returns `panels`, `activePanelId`, `activePanelKind`.
- `PUT /api/active-panel` can switch between wiki and canvas.
- `PUT /api/panels/:sessionId/:panelId/state` works for wiki state.

### UI verification

- Run MyOpenPanels Studio.
- Confirm bottom tabs render.
- Switch wiki/canvas multiple times.
- Rename Project and confirm title remains across tabs.
- Insert image via CLI while canvas tab is active, then switch away/back.
- Verify wiki panel does not trigger canvas selection saves.

## Migration

- Existing sessions remain valid.
- Existing canvas state files remain untouched.
- First bootstrap lazily creates missing wiki panel.
- Only commands advertised by the current CLI remain valid; compatibility aliases are removed.
- The only maintained skill entry point is `myopenpanels`.
- Panel-specific skills such as `myopenpanels-canvas` are intentionally removed; agents should always fetch the latest CLI-provided `agent context`.

## Open Questions

1. UI tab order: should `wiki` be first by default, or should old users land on `canvas` first for continuity?
2. Wiki display name: Chinese UI uses `文档库`; should English title be `Wiki`, `Docs`, or `Knowledge Base`?
3. Should active panel be context-local only, or should each Project remember its last active panel globally?
4. Should wiki markdown eventually be stored inside `.myopenpanels` only, or also export/sync to real project files such as `docs/`?
5. Should `agent context` default to markdown for agent readability, or JSON for easier machine routing?

## Recommended Defaults

- Default tab order: `文档库`, then `Canvas`.
- Active panel default: `wiki` for new Project.
- Active panel persistence: context-local for now, matching active session behavior.
- Entry skill: one stable `MyOpenPanels` skill only.
- CLI source of truth: `agent context` and the capability manifest generated by the latest Rust native CLI.
- Command surface: canonical namespace commands only, with no compatibility aliases.
