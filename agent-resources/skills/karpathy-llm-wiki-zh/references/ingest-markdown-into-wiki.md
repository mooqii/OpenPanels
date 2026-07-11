# 将源 Markdown 整合进 Karpathy LLM Wiki 中文版

当需要把一个源 Markdown 文档整合进结构化 Wiki 时，使用本参考流程。

工作流：

1. 领取任务。
2. 读取源 Markdown。
3. 读取 `wiki-conventions.md`，列出目标 Wiki Space 的页面，并检查 `SCHEMA.md`、
   `index.md`、`log.md`，以及可能变化的已有页面。
4. 如果这是首次摄取，先建立 `SCHEMA.md`、`index.md` 和 `log.md`，再新增知识页面。
   没有可用范围定义时，可根据源文档推断一个足够聚焦、后续易于调整的初始领域。
5. 对有价值的证据分类，在 `entities/`、`concepts/`、`comparisons/` 或
   `summaries/` 下创建或更新聚焦页面。
6. 为受影响页面补充综合、溯源、矛盾、边界条件、标签和有意义的相对 Markdown 链接。
7. 更新 `index.md`，让每个新页面都能在正确分区被发现。
8. 向 `log.md` 追加一条简短的摄取记录，列出所有变更路径。
9. 用当前 task id 写入每个变更页面。
10. 完成任务。

写作规则：

- 新生成的 Wiki 页面、摘要、索引项和日志记录使用中文。
- 整合，不堆放：每个源文档都应该加强已有 Wiki 图谱。
- 优先维护能跨多个源文档持续复用的稳定页面。
- 只有当概念、实体、比较或综合摘要有长期价值时才创建新页面；一篇来源的核心主题
  也可以满足这一条件。
- 不要创建 source page 或原始文档镜像页。原始文档已经由 Wiki 面板左侧列表维护。
- 当新证据确认、细化、反驳或取代旧说法时，更新已有页面。
- 用 `sourceDocumentIds` 和必要时简短的来源说明保留溯源；不要把原始 Markdown
  复制进页面。
- 使用 `wiki-conventions.md` 中的目录层级和页面约定。
- 保持页面聚焦、简洁、可导航。
- 不要重写无关页面。
- 不要因为切换到这个 skill 就翻译或重新生成整个 Wiki。

完成标准：

- 源 Markdown 中有价值的信息已综合进目标 Wiki Space。
- 相关 Wiki 页面反映了新证据。
- 当该任务首次初始化或实质改变 Wiki 时，`SCHEMA.md`、`index.md` 和 `log.md`
  已存在且彼此一致。
- 任务已完成。
