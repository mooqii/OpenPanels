# MyOpenPanels LLM Wiki 约定

每次创建或修改生成式 Wiki 页面时，都使用这些约定。

## 分层和目录结构

Wiki 面板左侧的原始文档列表是不可变的来源层。不要在生成式 Wiki 中创建
`raw/`、`sources/`，也不要为每一篇原始文档创建镜像页面。

生成式 Wiki 使用以下结构：

```text
SCHEMA.md
index.md
log.md
entities/<slug>.md
concepts/<slug>.md
comparisons/<slug>.md
summaries/<slug>.md
```

仅在某个目录第一次需要页面时创建它。不要强行创建空目录，也不要为一笔顺带
提及的信息创建页面。

- `entities/`：人物、组织、产品、项目、模型，以及其他可长期复用的具名对象。
- `concepts/`：主题、方法、机制、定义和反复出现的观点。
- `comparisons/`：具备长期价值的并列分析和取舍比较。
- `summaries/`：主题地图、时间线和跨来源综合；不适合归入单一概念或实体的内容。

## 基础页面

第一次摄取时，在新增知识页面前先创建以下根页面。对于已有 Wiki，保留仍有价值
的结构并更新这些页面，不要整体替换。

- `SCHEMA.md`：领域和范围；目录分类；文件名与链接约定；允许使用的标签分类；
  页面创建门槛；以及领域特有规则。
- `index.md`：按目录/类型分段、面向内容的目录。每个生成页面都应有一个 Markdown
  链接和一行摘要。保持最后更新时间和页面总数准确。
- `log.md`：只追加的记录。使用 `## [YYYY-MM-DD] action | subject`，然后列出该操作
  创建或更新的每个 Wiki 路径。

## 页面约定

- 使用小写、连字符命名的文件名和路径，例如
  `concepts/attention-mechanism.md`。
- 每个生成的知识页面都以 YAML 前置元数据开始：

  ```yaml
  ---
  title: 可读标题
  created: YYYY-MM-DD
  updated: YYYY-MM-DD
  type: entity | concept | comparison | summary
  tags: [受控, 标签]
  sourceDocumentIds: [raw-document-id]
  confidence: high | medium | low
  contested: false
  contradictions: []
  ---
  ```

- `sourceDocumentIds` 只用于保存溯源信息。绝不把原始 Markdown 复制进生成式 Wiki。
  仅在某个结论需要额外说明来源时补充简短来源注记。
- 标签必须来自 `SCHEMA.md` 中的分类；需要新标签时，先在那里新增。
- 用标准相对 Markdown 链接连接相关页面，例如
  `[注意力机制](../concepts/attention.md)`。`index.md` 的链接相对于根目录，例如
  `[注意力机制](concepts/attention.md)`。
- 新建或实质更新的页面在存在相关页面时应建立链接；不要为了凑数量而编造链接。
- 页面应聚焦且易于扫读。页面超过约 200 行时，拆为聚焦页面并更新链接和索引。

## 编辑政策

- 当一个主题是单篇来源的核心内容，或在多篇来源中重复出现时才创建页面；否则把
  有价值的细节整合进已有页面。
- 新证据确认、细化、反驳或取代旧内容时更新已有页面，同时更新 `updated` 和溯源。
- 不要静默抹去实质冲突。保留双方观点、日期和来源文档 ID；必要时设为
  `contested: true`，并在 `contradictions` 中列出关联页面路径。
- 保留仍有价值的用户页面和已有结构。不要因为切换技能就重新生成、重命名或翻译
  无关页面。
