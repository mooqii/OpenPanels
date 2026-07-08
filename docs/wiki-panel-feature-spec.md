# Wiki Panel Feature Development Spec

## 背景

OpenPanels 已经具备 Project 内多 panel 的基础形态，`wiki` panel 目前是占位 UI，`canvas` panel 已经可独立持久化和通过 CLI 被 agent 操作。本 spec 描述下一阶段的 wiki 面板：把用户或 agent 添加的原始文档整理为可编辑 Markdown，再持续沉淀为 agent 友好的结构化 wiki。

产品参考 Karpathy 的 LLM Wiki 思路：知识库分为原始来源、结构化 wiki、规则/schema 三层；新来源进入后，不只是被检索，而是被 agent 增量整合进持久 Markdown wiki，包括索引、摘要、分类、交叉引用、日志和冲突记录。

## 目标

1. Wiki 面板由两个主要模块组成：
   - 左侧：原始文档模块，管理用户或 agent 添加的源文档。
   - 右侧：结构化 wiki 模块，展示和编辑由 agent 整理后的 wiki。
2. 用户可以在原始文档模块上传任意文件类型，也可以直接新建 Markdown 文档；agent 也可以通过 CLI/API 添加原始文档。
3. 每个原始文档都生成一份纯 Markdown 文档：
   - `.md`、`.markdown`、`.txt` 等纯文本文件不需要转换，只做规范化保存。
   - 非纯文本文件进入转换任务；系统不限制文件类型，允许 agent 返回转换失败状态。
4. 新文档添加后，系统立即创建 agent 任务并真实唤醒可用的 agent thread，让 agent 把内容转换为 Markdown，并在转换完成后继续把内容整合到结构化 wiki。
5. 原始文档列表展示每个文档的转换状态：`转换中`、`转换失败`、`已转换`。已转换文档显示 Markdown 图标，点击后在当前页面打开 Markdown 阅读/编辑弹窗。
6. 结构化 wiki 内置默认整理规则，后续允许用户创建/选择自定义 wiki 规则。不同规则生成彼此独立的一套结构化 wiki。
7. 结构化 wiki 不支持用户直接上传文件，但允许用户查看、编辑和新建 Markdown wiki 页面。
8. agent 可以通过 CLI/API 读取任务、读取源文档、写回 Markdown、更新 wiki 页面，并获取当前 wiki 规则。

## 已确认产品决策

- 新文档添加后必须真唤醒 agent thread，让 agent 立即开始处理；这不是只把任务暴露在 `agent-context` 里。唤醒机制需要面向多种 agent host 设计，不能只支持 Codex。
- 结构化 wiki 写入默认自动应用，v1 不需要生成 diff 或等待用户确认。
- Wiki 页面先只存放在 `.myopenpanels` 内，不在 v1 同步到项目 `docs/` 目录。
- 新 Raw Document 默认只整理进添加当刻的 active Wiki Space。
- 每个已开启的 agent 处理进程都必须记录自己绑定的 Wiki Space。agent 处理中用户可能切换面板或切换 active Wiki Space，已开始的任务仍然写入任务创建或 claim 时绑定的 Wiki Space，不能跟随 UI 当前状态漂移。
- 原始文档模块不限制上传文件类型。无法转换的文件由 agent 返回 `转换失败`，并保留错误信息和重试入口。
- 用户在原始文档模块新建 Markdown 后，系统立即唤醒 agent 更新结构化 wiki。
- 用户保存 Source Markdown 后，系统立即唤醒 agent 重新整理结构化 wiki。
- 用户可以在结构化 wiki 模块新建 Markdown 页面。该页面直接写入当前 active Wiki Space，并立即触发 agent 更新 `index.md`、`log.md` 和相关链接。

## 非目标

- 不在 v1 内实现完整多人协作、权限系统或云同步。
- 不在 v1 内实现通用向量检索/RAG 服务。优先用 `index.md`、页面元数据和本地文本搜索支撑 agent 工作。
- 不在 v1 内要求所有文件类型都能转换成功。复杂格式可以由 agent 或后续插件尝试处理，失败时进入可见失败状态。
- 不让结构化 wiki 右侧成为第二个文件上传入口。
- 不强制用户审核每一次 agent 写入。v1 默认自动写入并保留日志，后续可加审核模式。
- 不在 v1 内实现结构化 wiki 写入 diff review。
- 不在 v1 内实现 wiki 页面到项目源码目录的双向同步。

## 核心概念

### Raw Document

原始文档。它是用户或 agent 添加的来源文件，是事实来源，不被 agent 改写。

### Source Markdown

每个 Raw Document 对应的一份规范化 Markdown。它用于承接转换结果，也允许用户打开弹窗查看和编辑。编辑并保存 Source Markdown 后，系统立即创建 ingest task 并唤醒 agent 重新整理结构化 wiki。

### Wiki Space

一套独立的结构化 wiki。每个 Wiki Space 绑定一个 Wiki Rule Set，有自己的 `index.md`、`log.md`、分类页面、主题页面、来源页面和摘要。

### Wiki Rule Set

wiki 的整理规则。包含分类方式、页面命名、frontmatter 约定、索引结构、日志格式、引用规范和 agent 工作流。系统提供默认规则，用户可以复制后自定义。

### Agent Task

由系统创建、由 agent 执行的持久任务。任务创建时会绑定目标 Wiki Space，agent 处理过程中不能改用用户后来切换到的 Wiki Space。主要类型：

- `convert_document_to_markdown`
- `ingest_markdown_into_wiki`
- `lint_wiki`
- `rebuild_wiki_index`

## 产品体验

### 面板布局

Wiki panel 使用左右分栏：

- 左侧宽度建议 320-400px，用于原始文档列表、添加入口、筛选和状态。
- 右侧占剩余空间，用于结构化 wiki 的目录、页面阅读/编辑、规则选择。
- 移动端降级为两个 tab：`原始文档` 和 `结构化 Wiki`。

### 左侧：原始文档模块

左侧顶部提供：

- 添加文档按钮，支持文件选择和拖拽上传，不限制文件类型。
- 新建 Markdown 按钮，用于直接创建 Source Markdown 文档。
- 搜索输入。
- 状态筛选：全部、转换中、转换失败、已转换、待入库、入库失败、已入库。

文档列表项显示：

- 文件名或文档标题。
- 文件类型、添加时间、来源：用户或 agent。
- 转换状态：
  - `等待转换`：任务已创建但尚未被 agent claim。
  - `转换中`：agent 已 claim 或正在写入结果。
  - `转换失败`：显示错误摘要和重试按钮。
  - `已转换`：显示 Markdown 图标。
  - `无需转换`：纯文本文件已作为 Markdown 保存，也显示 Markdown 图标。
- Wiki 入库状态可作为第二行较弱状态显示：
  - `待整理`、`整理中`、`整理失败`、`已整理`、`已过期`。

交互：

- 点击列表项选择文档，在右侧 wiki 中可高亮相关来源或页面。
- 点击 Markdown 图标，在当前页面打开 Markdown 阅读/编辑弹窗。
- 转换失败时可点击重试。
- 用户新建 Markdown 后，系统直接生成 Source Markdown，把该文档对创建当刻的 active Wiki Space 的 ingest 状态置为 `待整理`，创建 ingest task，并真实唤醒 agent。
- 用户保存 Source Markdown 后，系统把该文档对保存当刻的 active Wiki Space 的 ingest 状态置为 `待整理`，创建新的 ingest task，并真实唤醒 agent 重新整理结构化 wiki。
- 用户添加新 Raw Document 时，server 必须立即解析并记录当刻 active Wiki Space，后续转换完成后的自动 ingest 只能写入这个被记录的 Wiki Space。

### Markdown 阅读/编辑弹窗

弹窗用于 Source Markdown，不直接编辑原始文件。

功能：

- 默认阅读模式，可切换编辑模式。
- 显示标题、源文件名、转换状态、最后更新时间。
- 支持保存、取消、重新触发 wiki 整理。
- 保存时使用 `contentVersion` 做乐观并发控制；如果 agent 正在基于旧版本处理，相关任务标记为 `stale` 并重新排队。
- 保存成功后立即创建 ingest task 并唤醒 agent，不只是在 UI 中标记过期。

v1 编辑器可以使用 textarea + Markdown preview；后续可替换成更完整的 Markdown editor。

### 右侧：结构化 Wiki 模块

右侧顶部提供：

- Wiki Space 选择器。
- 当前规则名称。
- 编辑规则入口。
- 新建 Markdown 页面入口。
- 页面搜索。

切换 Wiki Space 只影响之后新建的 Raw Document 和手动触发的重新整理任务，不影响已经创建、已经 claim 或正在运行的 agent task。

主体建议拆成：

- 左侧目录树或索引栏：展示 `index.md` 分类结构、页面标题和摘要。
- 右侧页面阅读/编辑区：查看和编辑已有 wiki 页面。

用户允许的操作：

- 选择 Wiki Space。
- 查看 `index.md`、`log.md`、来源页面、主题页面、分类页面。
- 编辑已有 wiki 页面并保存。
- 新建 Markdown wiki 页面并保存到当前 active Wiki Space。
- 查看页面关联来源。
- 查看 agent 最近更新日志。

用户不允许的操作：

- 不允许在结构化 wiki 模块直接上传文件。
- 手动新建页面只支持 Markdown 页面，不支持直接上传二进制或富文本文件。

结构化 wiki 页面新建或保存后，server 直接写入页面文件，并创建 `rebuild_wiki_index` task 唤醒 agent 更新 `index.md`、`log.md`、反向链接和必要的分类摘要。

## 默认 Wiki 规则

默认规则的目标是让 wiki 对 agent 友好，而不是只对人类好看。

默认 Wiki Space 文件结构：

```text
wikis/
  <wikiSpaceId>/
    rules.md
    pages/
      index.md
      log.md
      overview.md
      sources/
        <source-slug>.md
      categories/
        <category-slug>.md
      topics/
        <topic-slug>.md
      entities/
        <entity-slug>.md
      claims/
        contradictions.md
```

默认规则要求：

- `index.md` 是内容导向索引，按分类列出页面链接、单句摘要、来源数量、更新时间。
- `log.md` 是按时间追加的操作日志，记录 ingest、query、lint、人工编辑和规则变更。
- 每个 Raw Document 至少生成一个 `sources/<source-slug>.md` 来源页面，包含摘要、关键点、引用、关联页面。
- 对稳定概念、人物、组织、产品、项目、地点等生成或更新 `topics/` 或 `entities/` 页面。
- 对重要冲突、过期信息、互相矛盾的来源，写入相关页面并在 `claims/contradictions.md` 建立索引。
- 页面尽量使用 wikilink 或相对 Markdown link，方便 agent 和人类导航。
- 每次 ingest 必须更新 `index.md` 和 `log.md`。

页面 frontmatter 建议：

```yaml
---
title: "Page title"
type: "source | topic | entity | category | overview | log"
summary: "One sentence summary"
tags: []
sourceDocumentIds: []
updatedAt: "2026-07-08T00:00:00.000Z"
---
```

## 自定义规则与独立 Wiki Space

用户可以基于默认规则创建新的 Wiki Rule Set。每个 Rule Set 可以生成一个新的 Wiki Space。

规则变更原则：

- 修改规则不直接改写旧 Wiki Space，避免用户已有 wiki 被意外重构。
- 用户可以选择：
  - 从当前规则继续维护已有 Wiki Space。
  - 基于新规则创建新的 Wiki Space。
  - 将指定 Raw Documents 重新 ingest 到新 Wiki Space。
- 每个 Wiki Space 记录创建时使用的 `ruleSetId` 和 `ruleSetVersion`。

自定义规则编辑内容：

- wiki 名称和说明。
- 分类体系。
- 页面命名规则。
- frontmatter 字段。
- index/log 格式。
- ingest 时是否生成来源页、主题页、实体页、矛盾索引。
- agent 写作风格和引用规则。

## 状态模型

Wiki panel state 不应存放大文本内容，只保存索引、元数据和当前 UI 状态。原始文件、Source Markdown、Wiki 页面作为 panel 文件或 assets 存储。

建议从现有 `schemaVersion: 1` 升级到 `schemaVersion: 2`：

```ts
export interface WikiStateV2 {
  schemaVersion: 2
  rawDocuments: RawDocument[]
  ruleSets: WikiRuleSet[]
  wikiSpaces: WikiSpace[]
  activeRawDocumentId: string | null
  activeWikiSpaceId: string | null
  activeWikiPagePath: string | null
  agentProcesses: AgentProcessContext[]
  tasks: AgentTaskSummary[]
}

export interface RawDocument {
  id: string
  title: string
  originalFileName: string
  mimeType: string
  sizeBytes: number
  sha256: string
  source: "user" | "agent"
  originalRef: string
  markdownRef: string | null
  markdownVersion: number
  conversion: ConversionState
  ingestionByWikiSpace: Record<string, IngestionState>
  createdAt: string
  updatedAt: string
}

export interface ConversionState {
  status:
    | "not_required"
    | "queued"
    | "converting"
    | "failed"
    | "ready"
  taskId: string | null
  error: string | null
  updatedAt: string
}

export interface IngestionState {
  status:
    | "not_started"
    | "queued"
    | "ingesting"
    | "failed"
    | "ingested"
    | "stale"
  taskId: string | null
  markdownVersion: number
  error: string | null
  updatedAt: string
}

export interface WikiRuleSet {
  id: string
  title: string
  description: string
  builtIn: boolean
  version: number
  rulesRef: string
  createdAt: string
  updatedAt: string
}

export interface WikiSpace {
  id: string
  title: string
  ruleSetId: string
  ruleSetVersion: number
  rootRef: string
  pageIndex: WikiPageIndexItem[]
  createdAt: string
  updatedAt: string
}

export interface WikiPageIndexItem {
  path: string
  title: string
  type: string
  summary: string
  tags: string[]
  sourceDocumentIds: string[]
  updatedAt: string
}

export interface AgentTaskSummary {
  id: string
  type:
    | "convert_document_to_markdown"
    | "ingest_markdown_into_wiki"
    | "lint_wiki"
    | "rebuild_wiki_index"
  status: "queued" | "claimed" | "running" | "failed" | "succeeded" | "stale"
  targetId: string
  wikiSpaceId: string | null
  ruleSetId: string | null
  ruleSetVersion: number | null
  markdownVersion: number | null
  claimedByProcessId: string | null
  error: string | null
  createdAt: string
  updatedAt: string
}

export interface AgentProcessContext {
  id: string
  agentHost: string
  threadId: string | null
  taskId: string | null
  wikiSpaceId: string
  status: "running" | "idle" | "finished" | "failed"
  startedAt: string
  updatedAt: string
}
```

`activeWikiSpaceId` 是 UI 当前选择；`AgentTaskSummary.wikiSpaceId` 和 `AgentProcessContext.wikiSpaceId` 是任务/进程绑定的目标 wiki。agent 写入页面时必须使用任务或进程记录里的 `wikiSpaceId`，不能重新读取 UI 当前 active Wiki Space 作为目标。

## 存储结构

建议 wiki panel 目录：

```text
.myopenpanels/
  sessions/
    <sessionId>/
      panels/
        <wikiPanelId>/
          panel.json
          state.json
          raw/
            <rawDocumentId>/
              meta.json
              original/
                <uploaded-file>
              source.md
          rules/
            default/
              rules.md
            <ruleSetId>/
              rules.md
          wikis/
            <wikiSpaceId>/
              pages/
                index.md
                log.md
                overview.md
                sources/
                categories/
                topics/
                entities/
                claims/
          tasks/
            <taskId>.json
          processes/
            <processId>.json
```

Wiki 页面 v1 只保存在 `.myopenpanels` 的 wiki panel 目录内。后续可以增加导出或同步到项目 `docs/` 的能力，但不能作为本阶段写入路径。

如果沿用现有 `assets/` 机制，需要支持嵌套路径和稳定引用。长期建议为 wiki 增加更语义化的 panel file helpers，不把 Markdown 页面伪装成图片 asset。

## Agent Push 机制

“立即主动 push 给 agent”是 v1 的必需能力：用户或 agent 添加新文档后，OpenPanels 必须唤醒当前可用的 agent thread，让 agent 立即开始处理。持久任务队列仍然是可靠性基础，但不能替代真实唤醒。

Agent wakeup 需要抽象为 host adapter，避免只绑定 Codex：

```ts
export interface AgentWakeupAdapter {
  host: string
  canWake(target: AgentThreadTarget): Promise<boolean>
  wake(target: AgentThreadTarget, message: AgentWakeupMessage): Promise<void>
}

export interface AgentThreadTarget {
  host: string
  threadId: string
  projectDir: string
  contextId: string
}

export interface AgentWakeupMessage {
  projectDir: string
  sessionId: string
  wikiPanelId: string
  taskId: string
  taskType: AgentTaskSummary["type"]
  documentId: string | null
  wikiSpaceId: string
}
```

流程：

1. agent host 或 CLI 在启动/进入项目时注册自己的 `AgentThreadTarget`，写入 context-local agent thread registry。
2. 用户或 agent 添加 Raw Document 后，server 解析当刻 active Wiki Space，并把 `wikiSpaceId` 写入 document ingestion state 和 task。
3. server 创建任务：
   - 任意非纯文本文件创建 `convert_document_to_markdown` task。
   - 原始文档模块中新建 Markdown 或上传纯文本文件时，直接创建 `ingest_markdown_into_wiki` task。
   - Source Markdown 保存后，创建新的 `ingest_markdown_into_wiki` task。
   - 结构化 wiki 模块中新建或保存 Markdown 页面后，创建 `rebuild_wiki_index` task。
4. task 写入 `tasks/<taskId>.json`，并更新 `state.json`。
5. server 通过 `AgentWakeupAdapter` 向已注册且可用的 agent thread 发送 wakeup message。message 必须包含 `projectDir`、`sessionId`、`wikiPanelId`、`taskId`、`taskType`、`documentId`、`wikiSpaceId`。
6. agent 被唤醒后运行 `agent-context` 或 `wiki tasks next`，claim task，并开始处理。
7. `convert_document_to_markdown` task complete 后，server 必须立即创建继承同一 `wikiSpaceId` 的 `ingest_markdown_into_wiki` task，并再次唤醒 agent 或让同一 agent process 继续处理。
8. 如果没有可唤醒的 agent thread，任务仍保持 `queued`，UI 显示等待 agent；一旦 agent 注册或刷新 `agent-context`，应立即看到 pending task。

Wakeup message 不包含完整文档内容，只包含任务定位信息，避免大 payload 和敏感内容散落到 agent thread 消息里。

任务 claim 规则：

- agent 执行前调用 `wiki task claim <taskId>`。
- task 从 `queued` 变为 `claimed` 或 `running`。
- claim 时创建或更新 `processes/<processId>.json`，并把 task 记录的 `wikiSpaceId` 写入进程上下文。
- agent 在后续所有 markdown/wiki 写入命令里必须携带 `taskId` 或 `processId`，server 用它解析固定的 `wikiSpaceId`。
- agent 写回成功后调用 complete。
- 如果输入文档或 Markdown 版本变化，旧 task 变为 `stale`。
- 失败时保存 error，UI 显示 `转换失败` 或 `整理失败` 并允许重试。

## API 设计

### Raw Documents

```http
GET /api/wiki/raw-documents
POST /api/wiki/raw-documents
GET /api/wiki/raw-documents/:documentId
GET /api/wiki/raw-documents/:documentId/original
GET /api/wiki/raw-documents/:documentId/markdown
PUT /api/wiki/raw-documents/:documentId/markdown
POST /api/wiki/raw-documents/:documentId/retry-conversion
POST /api/wiki/raw-documents/:documentId/enqueue-ingest
```

`POST /api/wiki/raw-documents` 支持 multipart upload，上传文件类型不做白名单限制；也支持 JSON 添加文本，用于原始文档模块中新建 Markdown：

```json
{
  "title": "Meeting notes",
  "fileName": "meeting-notes.md",
  "mimeType": "text/markdown",
  "source": "user",
  "content": "# Meeting notes\n..."
}
```

`wikiSpaceId` 可显式传入。未传入时，server 使用当前 context/process 记录的 active Wiki Space，并把解析结果固化到后续 task；用户之后切换 Wiki Space 不影响这个文档已经创建的 task。

如果上传文件不是纯文本且 agent 后续无法转换，document conversion 状态进入 `failed`，并保存 agent 返回的错误信息。

### Tasks

```http
GET /api/wiki/tasks
POST /api/wiki/tasks/:taskId/claim
POST /api/wiki/tasks/:taskId/complete
POST /api/wiki/tasks/:taskId/fail
```

`claim` 返回 `processId`、`taskId` 和固定的 `wikiSpaceId`。后续写 Markdown 或写 wiki page 时，agent 应携带 `taskId` 或 `processId`。

### Agent Wakeup and Processes

```http
GET /api/wiki/agent-targets
POST /api/wiki/agent-targets
DELETE /api/wiki/agent-targets/:targetId

GET /api/wiki/processes
GET /api/wiki/processes/:processId
PUT /api/wiki/processes/:processId
```

`POST /api/wiki/agent-targets` 用于注册可唤醒的 agent thread target。不同 agent host 可以用不同 adapter 实现 wakeup，但 server 对上层保持同一套 target/task 协议。

### Wiki Spaces and Pages

```http
GET /api/wiki/rule-sets
POST /api/wiki/rule-sets
PUT /api/wiki/rule-sets/:ruleSetId

GET /api/wiki/spaces
POST /api/wiki/spaces
PUT /api/wiki/spaces/:wikiSpaceId
GET /api/wiki/active-space
PUT /api/wiki/active-space

GET /api/wiki/spaces/:wikiSpaceId/pages
POST /api/wiki/spaces/:wikiSpaceId/pages
GET /api/wiki/spaces/:wikiSpaceId/pages/*pagePath
PUT /api/wiki/spaces/:wikiSpaceId/pages/*pagePath
```

Page write 需要支持 `expectedVersion` 或 `updatedAt`，避免覆盖用户刚做的编辑。

`POST /api/wiki/spaces/:wikiSpaceId/pages` 用于结构化 wiki 模块中新建 Markdown 页面。保存页面后 server 创建 `rebuild_wiki_index` task 并唤醒 agent。

## CLI 设计

CLI 是 agent 的主要入口。建议新增 namespace 命令：

```text
openpanels-local wiki context
openpanels-local wiki agent-target register --host <host> --thread-id <id>
openpanels-local wiki raw add --file <path> [--wiki-space-id <id>]
openpanels-local wiki raw new-markdown --title <title> --file-name <name> [--wiki-space-id <id>]
openpanels-local wiki raw add-text --title <title> --file-name <name> [--wiki-space-id <id>]
openpanels-local wiki raw list --format json
openpanels-local wiki raw read --document-id <id> --output <path>
openpanels-local wiki markdown read --document-id <id>
openpanels-local wiki markdown write --document-id <id> --file <path> --task-id <id>
openpanels-local wiki tasks list --status queued
openpanels-local wiki tasks next
openpanels-local wiki tasks claim --task-id <id>
openpanels-local wiki tasks complete --task-id <id>
openpanels-local wiki tasks fail --task-id <id> --message <message>
openpanels-local wiki processes list
openpanels-local wiki processes read --process-id <id>
openpanels-local wiki rules list
openpanels-local wiki rules read --rule-set-id <id>
openpanels-local wiki spaces list
openpanels-local wiki spaces active --wiki-space-id <id>
openpanels-local wiki pages list --wiki-space-id <id>
openpanels-local wiki pages create --wiki-space-id <id> --path <path> --file <path>
openpanels-local wiki pages read --wiki-space-id <id> --path index.md
openpanels-local wiki pages write --wiki-space-id <id> --path index.md --file <path> --task-id <id>
```

`agent-context --format markdown` 需要在 active panel 是 wiki 或存在 pending wiki tasks 时加入：

- 当前 Project、wiki panel、active Wiki Space。
- pending tasks 摘要。
- 当前 agent thread target 是否已注册、是否可唤醒。
- 规则读取命令。
- agent 应执行的下一步建议。

## Agent 工作流

### 添加原始文档

agent 可以通过 CLI 添加文档：

```bash
openpanels-local wiki raw add --project "$PWD" --file ./notes/report.pdf --format json
```

添加后系统不假设 agent 已完成转换，而是创建 task。上传文件不做类型限制；若 agent 无法转换，写回转换失败状态。若添加的是 Markdown/txt，系统直接写入 Source Markdown、创建 ingest task，并唤醒 agent 更新结构化 wiki。

无论由用户还是 agent 添加文档，目标 Wiki Space 都在添加时确定：默认使用当前 active Wiki Space，也允许 CLI/API 显式传入 `wikiSpaceId`。后续转换完成后自动创建的 ingest task 必须继承同一个 `wikiSpaceId`。

用户在原始文档模块新建 Markdown 等价于创建一个 `source: "user"` 的 Raw Document，`conversion.status` 为 `not_required` 或 `ready`，随后立即进入 ingest。

### 转换为 Source Markdown

agent 处理 `convert_document_to_markdown`：

1. 读取 task。
2. 读取原始文档或导出到临时路径。
3. 尽量保留标题、层级、列表、表格、引用和图片占位。
4. 不在转换阶段做跨文档综合，不更新结构化 wiki。
5. 使用 `taskId` 或 `processId` 写回 Source Markdown。
6. complete task。系统随后立即创建 ingest task，并继承原 task 的 `wikiSpaceId`。
7. 如果无法可靠转换，调用 fail，UI 显示 `转换失败`，保留错误摘要和重试入口。

### 整合到结构化 Wiki

agent 处理 `ingest_markdown_into_wiki`：

1. 读取 Source Markdown。
2. 从 task 或 process context 读取固定的目标 `wikiSpaceId`，不读取 UI 当前 active Wiki Space 作为写入目标。
3. 读取目标 Wiki Space 绑定的 Wiki Rule Set 的 `rules.md`。
4. 读取目标 Wiki Space 的 `index.md`、`log.md`、相关页面。
5. 创建或更新来源页面。
6. 更新相关分类、主题、实体页面。
7. 标记冲突、过期信息和待确认事实。
8. 更新 `index.md`。
9. 追加 `log.md`。
10. 自动应用写入结果。
11. complete task，并写入被更新页面列表。

agent 不应修改 Raw Document 原始文件。

### 新建或编辑结构化 Wiki 页面

用户在结构化 wiki 模块中新建 Markdown 页面时，server 直接把页面写入当前 active Wiki Space。随后系统创建 `rebuild_wiki_index` task 并唤醒 agent，要求 agent 更新 `index.md`、`log.md`、分类摘要、反向链接和必要的关联页。

用户编辑已有结构化 wiki 页面后同样触发 `rebuild_wiki_index` task。该任务绑定保存当刻的 Wiki Space。

## 并发与冲突

- Raw Document 原始文件不可变。
- Raw Document 创建时确定目标 Wiki Space；用户之后切换 active Wiki Space 不会改变已排队或运行中的 task。
- Source Markdown 保存时递增 `markdownVersion`。
- Source Markdown 保存后立即创建新的 ingest task 并唤醒 agent。
- Ingest task 记录启动时的 `markdownVersion`。
- Ingest task 记录目标 `wikiSpaceId`、`ruleSetId` 和 `ruleSetVersion`；agent 必须按 task target 写入。
- 如果任务完成前 Source Markdown 被用户修改，任务结果不能直接 complete，应标记 `stale` 或重新基于新版本处理。
- Wiki 页面保存使用乐观锁。若用户和 agent 同时编辑，server 返回冲突，agent 需要重新读取并合并。
- 所有 agent 写入都写入 `log.md`，并在 task result 中记录 updated pages。

## 错误处理

转换失败：

- 文档列表显示 `转换失败`。
- 保存 agent 错误消息和最近失败时间。
- 支持 retry，retry 创建新 task，不覆盖历史失败信息。

整理失败：

- Source Markdown 仍可查看和编辑。
- Wiki 入库状态显示 `整理失败`。
- 支持 retry。

规则缺失或损坏：

- UI 提示 Wiki Rule Set 无效。
- agent task fail，并提示用户修复规则或重建 Wiki Space。

## 实施阶段

### Phase 1: 数据模型与存储

- 升级 wiki state 到 `schemaVersion: 2`。
- 增加 Raw Document、Rule Set、Wiki Space、Task、Agent Process 元数据。
- 增加 wiki panel file helpers。
- 内置默认 Rule Set 和默认 Wiki Space。
- 增加 active Wiki Space 持久化，并确保 task 固化目标 `wikiSpaceId`。

验收：

- 新 Project 自动拥有默认 Wiki Space。
- 旧 `schemaVersion: 1` wiki state 可迁移。
- 大文本不写入 `state.json`。
- 新文档创建后，task 中记录的 `wikiSpaceId` 不随 UI 切换变化。

### Phase 2: 原始文档 UI 与 Markdown 弹窗

- 实现左右布局。
- 支持用户上传任意文件类型。
- 支持用户在原始文档模块中新建 Markdown。
- 纯文本文件直接变为 Source Markdown。
- 文档列表显示转换状态和 Markdown 图标。
- 实现 Markdown 阅读/编辑弹窗。

验收：

- 上传 `.md` 后立即显示 Markdown 图标。
- 上传任意未知文件类型后显示 `等待转换` 或 `转换中`，并允许后续进入 `转换失败`。
- 新建 Markdown 后立即创建 ingest task 并唤醒 agent。
- 保存 Source Markdown 后立即创建 ingest task 并唤醒 agent。

### Phase 3: Agent task queue 与 CLI

- 实现 task 创建、claim、complete、fail。
- 实现 agent thread target 注册和 host wakeup adapter。
- 实现 process context，并在 claim 后记录固定 `wikiSpaceId`。
- 实现 raw/markdown/rules/pages CLI 命令。
- 更新 `agent-context` 输出 pending wiki tasks。

验收：

- agent 添加文档后，studio 立即看到新 Raw Document。
- 用户添加文档后，可唤醒的 agent thread 被真实唤醒，并能通过 CLI 看到待处理 task。
- agent 写回 Markdown 后，UI 状态变为已转换。
- agent 写回转换失败后，UI 状态变为转换失败并可重试。
- 转换成功后，系统立即创建 ingest task 并继续唤醒 agent 更新结构化 wiki。
- agent 处理过程中用户切换 active Wiki Space，agent 仍写入 task 绑定的 Wiki Space。

### Phase 4: 结构化 Wiki 生成与编辑

- 实现 Wiki Space 页面读取/写入。
- 实现 `index.md` 驱动的目录视图。
- 支持查看、编辑和新建 Markdown wiki 页面。
- 默认规则驱动 agent ingest，写入结果默认自动应用。

验收：

- 一个 Markdown 文档 ingest 后，至少生成来源页、更新 `index.md` 和 `log.md`。
- 右侧能查看、编辑和新建 Markdown 页面。
- 结构化 wiki 模块不提供文件上传入口。
- ingest 完成后页面直接更新，不进入 diff review 队列。
- 新建或编辑结构化 wiki 页面后，系统立即创建 `rebuild_wiki_index` task 并唤醒 agent。

### Phase 5: 自定义规则与多 Wiki Space

- 支持复制默认规则为自定义规则。
- 支持基于规则创建独立 Wiki Space。
- 支持选择 active Wiki Space。
- 支持把指定 Raw Document 重新 ingest 到另一个 Wiki Space。

验收：

- 同一批 Raw Documents 可以生成两套独立 wiki。
- 切换 Wiki Space 不丢失页面编辑状态。
- 规则修改不会意外重写旧 Wiki Space。

## 测试计划

### Unit Tests

- pure text 检测：`.md`、`.markdown`、`.txt` 跳过转换。
- unknown file type 检测：不拒绝上传，进入转换队列。
- state migration：v1 到 v2。
- task lifecycle：queued -> claimed -> running -> succeeded/failed/stale。
- task target：创建 task 时固化 `wikiSpaceId`、`ruleSetId`、`ruleSetVersion`。
- process context：claim 后记录固定 `wikiSpaceId`。
- Source Markdown save：递增 `markdownVersion` 并创建 ingest task。
- markdown version：编辑后旧 ingest task stale。
- wiki page path sanitization：阻止路径逃逸。

### Server Tests

- `POST /api/wiki/raw-documents` 写入文件和 state。
- `POST /api/wiki/raw-documents` 不限制文件扩展名或 MIME type。
- 原始文档新建 Markdown 后创建 ingest task 并调用 wakeup adapter。
- `GET/PUT markdown` 读写 Source Markdown。
- `PUT markdown` 保存后创建 ingest task 并调用 wakeup adapter。
- task claim/complete/fail 更新状态。
- conversion fail 更新文档转换状态并保存错误消息。
- agent target register 后，创建 task 会调用 wakeup adapter。
- agent wakeup message 包含 task 定位信息但不包含完整文档内容。
- active Wiki Space 切换不影响已创建 task 的 `wikiSpaceId`。
- `POST /api/wiki/spaces/:wikiSpaceId/pages` 新建 Markdown 页面，并创建 `rebuild_wiki_index` task。
- page read/write 使用 optimistic concurrency。
- rule set 和 wiki space API 可创建、读取、切换。

### CLI Tests

- `wiki raw add` 添加文件并返回 document id。
- `wiki raw new-markdown` 创建 Source Markdown 并触发 ingest。
- `wiki agent-target register` 注册当前 agent thread target。
- `wiki tasks next` 返回 pending task。
- `wiki tasks claim` 返回 `processId` 和固定 `wikiSpaceId`。
- `wiki markdown write` 完成转换并触发 ingest task。
- `wiki tasks fail` 可把转换任务写为失败。
- `wiki pages create` 新建结构化 Markdown 页面并触发 index rebuild。
- `wiki pages write` 更新页面并刷新 page index。
- `agent-context` 在有 pending task 时包含 wiki 操作说明。

### UI Verification

- 上传 Markdown，立即可打开弹窗查看。
- 上传 PDF 或未知文件，状态进入转换队列。
- agent 返回失败后，文档状态显示 `转换失败` 并可重试。
- 上传文件后，可唤醒 agent thread 收到任务通知。
- 在原始文档模块新建 Markdown 后，可唤醒 agent thread 收到 ingest 任务通知。
- 模拟 agent 写回 Markdown，列表状态变为已转换。
- 保存 Source Markdown 后，wiki 入库状态变为待整理，并立即唤醒 agent。
- 模拟 agent 更新 `index.md` 和来源页，右侧目录和页面刷新。
- agent 处理期间切换 Wiki Space，结果仍写入原任务绑定的 Wiki Space。
- 新建和编辑已有 wiki 页面并保存。
- 新建或编辑结构化 wiki 页面后，agent 收到 `rebuild_wiki_index` 任务通知。
- 创建第二个 Wiki Space，确认两套页面独立。

## 待确认问题

1. 具体 agent host adapter 的实现顺序：在统一 `AgentWakeupAdapter` 协议下，首个落地版本需要接入哪些 host 的真实唤醒 API 或本地桥接能力。
2. 结构化 wiki 模块中新建 Markdown 页面时，是否需要提供页面模板选择，例如 `source`、`topic`、`entity`、`category`，还是 v1 只提供空白 Markdown。
