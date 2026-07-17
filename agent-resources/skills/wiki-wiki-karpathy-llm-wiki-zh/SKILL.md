---
id: karpathy-llm-wiki-zh
title: Karpathy LLM Wiki 中文
description: 用于创建、扩充、编辑或维护持久化、结构化的 Karpathy 风格 LLM Wiki，并以中文生成新的 Wiki 内容。
source: builtin
appliesTo:
  - wiki
taskTypes:
  - ingest_markdown_into_wiki
  - maintain_wiki
requiresCommands:
  - task.claim
  - task.heartbeat
  - task.complete
  - task.fail
  - wiki.raw.list
  - wiki.raw.read
  - wiki.raw.update
  - wiki.page.list
  - wiki.page.read
  - wiki.page.create
  - wiki.page.update
loadWhen:
  - 当前 Wiki 任务需要用中文维护由 LLM 生成的 Karpathy 风格 Wiki。
tokens: medium
---

当当前 MyOpenPanels Wiki 任务需要基于精选源文档创建、扩充、编辑或维护一个持久、
持续积累的 Wiki，并要求新生成的 Wiki 内容使用中文时，使用此技能。

此技能借鉴 Andrej Karpathy 的 LLM Wiki 理念，并针对 MyOpenPanels 作了调整：左侧的
原始文档列表始终作为事实来源层，LLM 则以生成式综合层的形式，逐步构建并维护一个
相互链接的 Markdown Wiki。Wiki 应随时间持续积累综合成果，而不是针对每个问题都
从头重新发现知识。

此技能仅用于编写 Wiki。它规定如何创建和维护生成式 Wiki；读取或使用已经完成的
Wiki 应由其他技能负责。

任务路由：

- `ingest_markdown_into_wiki`：读取 `references/ingest-markdown-into-wiki.md`。
- `maintain_wiki`：读取 `references/maintain-wiki.md`。

凡是需要写入生成式 Wiki 页面的任务，都必须先读取
`references/wiki-conventions.md`。

核心规则：

- 新生成的 Wiki 页面、摘要、索引项和日志记录必须使用中文。
- 将左侧原始文档列表视为原始来源层；除非任务要求生成摘要或 Wiki 页面，否则不要
  在 Wiki 页面中镜像原始来源，也不要翻译原始来源内容。
- 将 Wiki 视为由 LLM 管理的生成层，其中包含 `SCHEMA.md`、`index.md`、`log.md`，
  以及结构化的实体、概念、比较和摘要页面。
- 将每个来源整合进现有 Wiki，不要堆放彼此孤立的笔记。
- 不要创建唯一用途只是代表某一篇原始文档的页面。
- 当新证据改变现有认识时，更新交叉链接、矛盾、过时说法和综合结论。
- 在生成页面中使用原始文档 ID 保留来源溯源，不要复制原始 Markdown。
- 保持 `SCHEMA.md`、`index.md` 和 `log.md` 与当前 Wiki 一致。
- 不要仅仅因为选中了此技能，就重写、翻译或重新生成整个 Wiki。只更新当前任务所需
  的页面。
- 不要编造来源内容。

完成标准：

- 已遵循所选任务对应的参考工作流。
- 当前任务新生成的 Wiki 内容使用中文。
- Wiki 仍可通过索引页面和交叉链接进行导航。
- 所有相关 Markdown 源文档或 Wiki 页面的写入操作都包含当前任务 ID。
- 任务已由智能体标记为完成或失败，或已由桥接器托管的执行器结束。
