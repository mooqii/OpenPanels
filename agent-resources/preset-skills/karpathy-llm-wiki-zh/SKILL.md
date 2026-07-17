---
name: karpathy-llm-wiki-zh
description: 用于创建、扩充、编辑或维护持久化、结构化的 Karpathy 风格 LLM Wiki，并以中文生成新的 Wiki 内容。
---

使用此技能基于精选源文档创建、扩充、编辑或维护一个持久、持续积累的中文 Wiki。

此技能借鉴 Andrej Karpathy 的 LLM Wiki 理念：提供的文档始终作为事实来源层，模型
则以综合层的形式逐步构建并维护一个相互链接的 Markdown Wiki。Wiki 应随时间持续
积累综合成果，而不是针对每个问题都从头重新发现知识。

此技能仅用于编写 Wiki。它规定如何创建和维护生成式 Wiki；读取或使用已经完成的
Wiki 应由其他技能负责。

工作流路由：

- 整合新的来源时，读取 `references/ingest-markdown-into-wiki.md`。
- 修复或整理已有 Wiki 时，读取 `references/maintain-wiki.md`。

凡是需要写入生成式 Wiki 页面的任务，都必须先读取
`references/wiki-conventions.md`。

核心规则：

- 新生成的 Wiki 页面、摘要、索引项和日志记录必须使用中文。
- 将提供的文档视为来源层；不要在 Wiki 页面中镜像完整来源，也不要无故翻译来源
  内容。
- 将 Wiki 视为由 LLM 管理的生成层，其中包含 `SCHEMA.md`、`index.md`、`log.md`，
  以及结构化的实体、概念、比较和摘要页面。
- 将每个来源整合进现有 Wiki，不要堆放彼此孤立的笔记。
- 不要创建唯一用途只是代表某一篇原始文档的页面。
- 当新证据改变现有认识时，更新交叉链接、矛盾、过时说法和综合结论。
- 在生成页面中使用稳定的来源标识保留溯源，不要复制完整来源文本。
- 保持 `SCHEMA.md`、`index.md` 和 `log.md` 与当前 Wiki 一致。
- 不要仅仅因为选中了此技能，就重写、翻译或重新生成整个 Wiki。只更新当前目标所需
  的页面。
- 不要编造来源内容。

完成标准：

- 已遵循当前目标对应的参考工作流。
- 新生成的 Wiki 内容使用中文。
- Wiki 仍可通过索引页面和交叉链接进行导航。
