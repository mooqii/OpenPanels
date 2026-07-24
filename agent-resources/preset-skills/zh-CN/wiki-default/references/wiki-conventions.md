# Wiki 生成规范

创建或修改生成的 Wiki 页面时，始终遵循本规范。

## 分层与结构

提供的文档集合是来源层。不要在生成 Wiki 中创建 `raw/`、`sources/` 或逐文档镜像页面。

生成 Wiki 使用以下结构：

```text
SCHEMA.md
index.md
log.md
entities/<slug>.md
concepts/<slug>.md
comparisons/<slug>.md
summaries/<slug>.md
```

某类页面第一次出现时再创建对应目录，不要强制创建空目录，也不要为每次顺带提及都创建页面。

- `entities/`：人物、组织、产品、项目、模型或其他具有持续价值的具名对象。
- `concepts/`：主题、方法、机制、定义和反复出现的观念。
- `comparisons/`：具有持续价值的并列分析与权衡。
- `summaries/`：无法归入单一概念或实体的主题地图、时间线与跨来源综合。

## 基础页面

第一次整合来源时，先创建以下根页面，再添加知识页面。已有 Wiki 应保留有价值的结构并更新这些页面，不要整体替换。

- `SCHEMA.md`：记录领域与范围、目录分类、文件名和链接规范、允许的标签体系、建页门槛以及领域特有规则。
- `index.md`：按内容分区的目录。每个生成页面都应在所属目录或类型下拥有 Markdown 链接和单行摘要，并保持最后更新时间与页面数准确。
- `log.md`：只追加的记录。使用 `## [YYYY-MM-DD] action | subject`，随后列出本次操作创建或更新的每条 Wiki 路径。

## 页面规范

- 使用小写连字符文件名和路径，例如 `concepts/attention-mechanism.md`。
- 每个生成的知识页面都以前言开头：

  ```yaml
  ---
  title: 便于阅读的标题
  created: YYYY-MM-DD
  updated: YYYY-MM-DD
  type: entity | concept | comparison | summary
  tags: [controlled, tags]
  sourceIds: [stable-source-id]
  confidence: high | medium | low
  contested: false
  contradictions: []
  ---
  ```

- `sourceIds` 只记录出处。不要把完整来源复制进生成 Wiki；只有当论断来源需要额外背景时，才添加简短的来源注释。
- 标签必须来自 `SCHEMA.md` 的标签体系；使用新标签前先把它加入体系。
- 使用标准相对 Markdown 链接连接相关页面，例如 `[注意力](../concepts/attention.md)`。索引中的链接相对于根目录，例如 `[注意力](concepts/attention.md)`。
- 新建或实质更新的页面应在确有相关页面时建立链接，不要为了满足数量而虚构关联。
- 保持页面聚焦且便于扫读。页面超过约 200 行时，将其拆成聚焦页面并更新链接和索引。

## 编辑原则

- 某主题是一份来源的核心，或在多个来源中反复出现时，可以独立成页；否则把有价值的细节并入已有页面。
- 新证据确认、细化、反驳或取代旧论断时，更新已有页面的 `updated` 日期和出处。
- 不要悄悄抹除重要冲突。保留带日期与来源文档标识的双方观点，将 `contested` 设为 `true`，并在适用时把相关页面路径写入 `contradictions`。
- 保留有价值的用户页面和结构。不要因为所选 Skill 变化而重新生成、重命名或翻译无关页面。
