# Agent Guidance Protocol Spec

## 背景

OpenPanels 现在的正确方向是：只保留一个稳定入口 skill：
`skills/myopenpanels/SKILL.md`。这个 skill 不承载具体面板工作流，而是让
agent 使用最新的 `openpanels-local` CLI，再由 CLI 提供当前项目、面板、能力和
任务引导。

当前实现里 `agent-context` 已经从 skill 转移到了 CLI，但还有两个问题：

- 默认上下文里混入了过长的 canvas/wiki 操作说明。
- agent-facing 指令、状态摘要、长引导文本仍然混在同一个 TS 文件里。

目标方案是：**默认 context 给 agent 一次性完整指令集、核心能力和当前状态；
复杂场景再按需加载独立 guide/skill。**

## 设计原则

- CLI 是 agent 协议的唯一权威来源。
- agent 不需要知道内部函数，例如 `getProjectBootstrap`、
  `insertPlaceholder`、`writeWikiPage`。
- agent 看到的是稳定 intent、CLI 命令模板、参数 schema、当前状态和可加载
  guides。
- `agent context` 默认输出 markdown。这里不需要 JSON，因为当前 agent 对结构化
  markdown 的理解足够准确。
- 功能指令集可以一次性给全，避免 agent 频繁往返查询。
- 长提示词、复杂流程、类似 skill 的指导必须渐进加载。
- v1 不支持项目级自定义 guides，但协议保留 `source` 字段，方便以后引入用户
  自定义 guide/skill。

## 非目标

- 不引入 CLI 到 agent 的常驻连接。
- 不让浏览器面板直接向 agent 注入提示词。
- 不要求 agent 阅读 OpenPanels 源码。
- 不把本地 control/runtime 函数暴露为 agent API。
- v1 不支持项目目录下自定义 guide 覆盖内置 guide。

## 通信模型

OpenPanels 和 agent 的通信保持 pull-based：

```text
agent -> shell command -> openpanels-local
openpanels-local -> stdout markdown -> agent
studio/ui -> local server/local storage -> CLI-readable state
```

启动面板之后，CLI 不主动推送上下文给 agent。agent 在需要时主动拉取：

```bash
openpanels-local studio start --project "$PWD" --format json
openpanels-local agent context --project "$PWD"
openpanels-local agent guide wiki.index-document --project "$PWD" --task-id <task-id>
```

保留旧命令作为 alias：

```bash
openpanels-local agent-context --project "$PWD"
```

alias 规则：

- `agent-context` 等价于 `agent context`。
- `wiki context` 可以作为 wiki 优先视角的 alias，但默认也只输出短 context。
- 所有 context alias 都不默认输出长 guide body。

## Agent Context 输出内容

`agent context` 默认输出 markdown，并且应保持为一个结构化协议文档，而不是随意
散文。它包含五类信息：

1. 当前项目、active panel、可用 panels。
2. 当前状态摘要，例如 wiki pending task、canvas selection summary。
3. 完整 capability 指令集，包含 intent、命令模板和参数 schema。
4. 当前建议下一步命令。
5. 可按需加载的 guides/skills 列表。

### 示例结构

````markdown
# OpenPanels Agent Context

Protocol version: 1
CLI version: 0.1.x
Project: Project 1 (session:...)
Active panel: wiki (文档库)

## Panels

- * wiki: 文档库 (panel:...)
- canvas: Canvas (panel:...)

## State

### Wiki

- language: zh-CN
- pending task count: 2
- next task: ingest_markdown_into_wiki / queued / task:...

### Canvas

- has selection: true
- selected shape count: 1
- selected image asset: true

## Capabilities

### `wiki.task.next`

Read the next wiki task.

Command:

```bash
openpanels-local wiki tasks next --project "$PWD" --format json
```

Arguments:

| Name | Required | Type | Description |
| --- | --- | --- | --- |
| project | no | path | Defaults to `$PWD`. |

Output:

- JSON task object or `null`.

Related guides:

- `wiki.index-document`
- `wiki.convert-document`

### `canvas.image.insert`

Insert a local image into the canvas.

Command:

```bash
openpanels-local insert-image --project "$PWD" --image <path> --placement right --format json
```

Arguments:

| Name | Required | Type | Values | Description |
| --- | --- | --- | --- | --- |
| image | yes | path | | Local image path. |
| placement | no | enum | right, left, below | Placement relative to anchor/current canvas. |
| replace-shape-id | no | string | | Replace an existing placeholder/image shape. |

Related guides:

- `canvas.image-generation`

## Suggested Next Commands

```bash
openpanels-local wiki tasks next --project "$PWD" --format json
openpanels-local agent guide wiki.index-document --project "$PWD" --task-id <task-id>
```

## Available Guides

| ID | Source | Applies To | Load When |
| --- | --- | --- | --- |
| `canvas.image-generation` | builtin | canvas | User asks to generate or edit an image. |
| `wiki.index-document` | builtin | wiki | Task type is `ingest_markdown_into_wiki`. |
````

## Capability 指令集

Context 中直接给完整 capability 列表。每个 capability 是 agent-facing contract，
包含：

- `intent`：稳定 ID。
- `description`：一句话说明。
- `command`：当前 CLI 命令模板。
- `args`：参数 schema。
- `output`：返回结果摘要。
- `relatedGuides`：需要复杂引导时可加载的 guide。

v1 初始 capability：

```text
agent.context.read
agent.guides.list
agent.guide.read
studio.start
studio.status
studio.stop
panel.list
panel.switch
panel.state.read
canvas.state.read
canvas.selection.read
canvas.selection.asset.read
canvas.placeholder.create
canvas.image.insert
wiki.context.read
wiki.task.list
wiki.task.next
wiki.task.claim
wiki.task.complete
wiki.task.fail
wiki.raw.add
wiki.source.read
wiki.source.write
wiki.page.list
wiki.page.read
wiki.page.write
wiki.space.list
wiki.space.switch
```

当前 Rust 实现中，capability manifest 由 CLI crate 维护：

```text
crates/openpanels-local/src/agent.rs
```

后续如果需要给前端复用能力列表，再把这部分拆成语言无关 JSON。

## Guide 文件

长引导放在顶层 `agent-guides/`，方便编辑、review、版本管理：

```text
agent-guides/
  canvas.image-generation.md
  canvas.selection-reference.md
  wiki.task-intake.md
  wiki.convert-document.md
  wiki.index-document.md
  wiki.rebuild-index.md
```

Rust CLI 随 release binary 发布这些 guide。这样 agent 永远通过最新 CLI 获取和
当前 CLI 版本匹配的 guide。

当前实现文件：

```text
crates/openpanels-local/src/agent.rs
  定义 capability manifest、guide metadata、context/guide 渲染。
```

### Guide Frontmatter

Guide 是 markdown 文件，使用 frontmatter 描述元信息：

```markdown
---
id: wiki.index-document
title: Create Structured Wiki From Source Markdown
source: builtin
appliesTo:
  - wiki
taskTypes:
  - ingest_markdown_into_wiki
requiresCapabilities:
  - wiki.task.claim
  - wiki.source.read
  - wiki.page.write
  - wiki.task.complete
tokens: medium
---

You are indexing one source markdown document into the structured wiki.

Before writing:
- Claim the task.
- Read the source markdown.
- Read the target wiki space and relevant existing pages.

Writing rules:
- Use the wiki generation language from context.
- Preserve useful existing wiki structure.
- Add or update pages with concise, sourced content.
- Update indexes when needed.

Completion:
- Write changed pages.
- Mark the task complete.
```

`source` v1 固定为 `builtin`。后续版本可以允许：

```yaml
source: project
source: user
```

## Progressive Loading

默认 context 不输出完整 workflow。agent 只有在要执行具体复杂任务时才加载 guide。

### Canvas 图片生成

默认 context 给出：

- canvas capability 全量指令。
- 当前是否有 selection。
- guide `canvas.image-generation` 可用。

当用户要求生成或编辑图片时，agent 再运行：

```bash
openpanels-local agent guide canvas.image-generation --project "$PWD"
```

guide 输出：

- 当前 selection 摘要。
- 如果有选中图片，如何读取参考像素。
- 如何创建 placeholder。
- 如何调用 agent 自身图像模型。
- 如何用 `--replace-shape-id` 替换 placeholder。
- 为什么不要刷新 browser。

### Wiki 结构化索引

默认 context 给出：

- pending wiki task 数量。
- next task 摘要。
- task type 对应 guide。

agent 工作流：

```bash
openpanels-local wiki tasks next --project "$PWD" --format json
openpanels-local agent guide wiki.index-document --project "$PWD" --task-id <task-id>
```

guide 根据 `--task-id` 自动注入：

- taskId。
- taskType。
- documentId。
- wikiSpaceId。
- wikiLanguage。
- 相关读取/写入命令。
- 当前任务完成标准。

v1 不支持手动覆盖复杂实体，例如 `--document-id`、`--wiki-space-id`。这些都由
task 或当前上下文自动推导，避免上下文不一致。

## Agent Guide 命令

推荐命令：

```bash
openpanels-local agent guides --project "$PWD"
openpanels-local agent guide <guide-id> --project "$PWD"
openpanels-local agent guide <guide-id> --project "$PWD" --task-id <task-id>
```

`agent guides` 输出 compact markdown 列表：

```markdown
# OpenPanels Agent Guides

| ID | Source | Applies To | Task Types | Load When |
| --- | --- | --- | --- | --- |
| `canvas.image-generation` | builtin | canvas | | User asks to generate or edit an image. |
| `wiki.index-document` | builtin | wiki | ingest_markdown_into_wiki | A wiki indexing task is queued. |
```

`agent guide <id>` 输出完整 guide/skill body，并在顶部注入动态上下文：

````markdown
# Guide: wiki.index-document

Source: builtin
Applies to: wiki

## Current Context

- project: Project 1 (session:...)
- active panel: wiki
- task id: task:...
- task type: ingest_markdown_into_wiki
- document id: raw:...
- wiki space id: wiki:default
- wiki language: zh-CN

## Commands For This Task

```bash
openpanels-local wiki tasks claim --project "$PWD" --task-id task:... --format json
openpanels-local wiki markdown read --project "$PWD" --document-id raw:... --format json
openpanels-local wiki pages write --project "$PWD" --wiki-space-id wiki:default --path <page-path> --file <md-file> --task-id task:... --format json
openpanels-local wiki tasks complete --project "$PWD" --task-id task:... --format json
```

## Instructions

...guide markdown body...
````

## CLI 与 Agent 的完整流程

### 首次打开面板

```bash
openpanels-local studio start --project "$PWD" --format json
```

CLI 返回 `browserUrl`、`serverUrl`、`contextId`、`projectDir`、`storageDir` 等。
agent 打开 `browserUrl`。之后 agent 再读取 context：

```bash
openpanels-local agent context --project "$PWD"
```

### 普通面板操作

agent 根据 context 中的 capabilities 直接运行具体命令，例如：

```bash
openpanels-local selection --project "$PWD" --format json
openpanels-local panel-state --project "$PWD" --kind wiki --format json
```

这些功能命令可以继续使用 JSON 输出，因为它们是具体数据结果；但
`agent context` 自身默认是 markdown。

### 复杂任务

如果 context 显示某个 guide 适用，agent 先加载 guide：

```bash
openpanels-local agent guide <guide-id> --project "$PWD" [--task-id <task-id>]
```

然后按照 guide 执行具体 CLI 命令。完成后再次读取 context：

```bash
openpanels-local agent context --project "$PWD"
```

## Guide 编写规则

- 一个 guide 只覆盖一个复杂工作流。
- 不重复全局启动流程；默认 context 已经给出基础命令。
- 不引用内部 TS 函数名。
- 使用 capability intent 和 CLI 命令模板。
- 明确任务完成标准。
- 对失败处理只写和该 workflow 有关的部分。
- guide 可以较长，但只有被显式加载时才输出。
- 默认 context 永远不要内嵌完整 guide body。

## 实施计划

### Phase 1：协议拆分

- 新增 `openpanels-local agent context`。
- 保留 `agent-context` alias。
- 默认 context 改为短 markdown。
- 新增完整 capability manifest，context 中一次性输出能力、命令、参数 schema。
- 从 `agent-context.ts` 移除长 workflow 文本。

### Phase 2：Guide 文件化

- 新增顶层 `agent-guides/`。
- 把 canvas/wiki 长指导迁入 markdown guide 文件。
- build 时复制 guides 到 CLI 包。
- 新增 `agent guides` 和 `agent guide <id>`。
- guide frontmatter 支持 `source: builtin`。

### Phase 3：任务感知渲染

- `agent guide <id> --task-id <task-id>` 自动解析 wiki task。
- wiki guide 注入 task/document/wikiSpace/language。
- canvas guide 自动注入当前 selection summary。

### Phase 4：未来扩展预留

- 协议保留 `source`，后续支持 `project` 或 `user` guides。
- 可以增加 `tokens`、`variant` 或 `level`，支持 quick/full skill。
- 可以增加 `agent capability <intent>`，用于后续按需查看更详细 schema，但 v1
  context 已经直接输出完整 capability schema。

## 验收标准

- `openpanels-local agent context --project "$PWD"` 默认输出 compact markdown。
- context 包含完整 capability 指令集、命令模板和参数 schema。
- context 包含当前状态摘要和可用 guides，但不包含完整长 guide body。
- `openpanels-local agent guides` 能列出内置 guides，并显示 `source: builtin`。
- `openpanels-local agent guide <id>` 能输出完整 guide。
- `openpanels-local agent guide <id> --task-id <task-id>` 能注入任务上下文。
- 顶层 `agent-guides/*.md` 是编辑 agent 引导的主要入口。
- release binary 包含内置 guides。
- agent-facing 输出不暴露内部实现函数作为 API。
