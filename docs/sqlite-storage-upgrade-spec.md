# OpenPanels SQLite Storage Upgrade Spec

## 背景

当前 OpenPanels 的项目数据主要保存在 `.myopenpanels/` 下的 JSON 文件：

```text
.myopenpanels/
  index.json
  contexts/
    <contextId>/
      active-session.json
      active-panel.json
      agent-targets.json
      ...
  sessions/
    <sessionId>/
      session.json
      artifacts.json
      panels/
        <panelId>/
          panel.json
          state.json
          selection.json
          assets/
          raw/
          rules/
          tasks/
          wikis/
```

这种方式透明、容易调试，但会逐渐暴露三个问题：

1. `state.json` 和 `artifacts.json` 是整文件读写，小修改也会触发整份 JSON 解析和序列化。
2. schema 升级主要靠各处 `normalize*` 函数兜底，缺少统一 migration 记录。
3. 列表查询、排序、按状态过滤任务、按 panel 查数据等能力需要扫描文件或加载整份 state。

CreartCLI 的数据库层提供了一个更适合本地项目的参考模型：用 SQLite 作为持久化容器，启用 WAL，把文档主体作为 JSON snapshot 存在表里，同时抽出少量高频索引字段。OpenPanels 这次升级沿用这个方向，但要保留当前 panel/runtime 抽象。

## 目标

1. 使用 SQLite 管理 OpenPanels 项目元数据、panel 状态、artifact、selection、wiki task 等结构化数据。
2. 用 migration 管理数据库 schema 版本，所有结构升级都通过迁移脚本完成。
3. 保持现有 `OpenPanelsStorage` / `OpenPanelsRuntime` 对外接口基本不变，优先替换 local storage 实现。
4. 保留大文件在文件系统中的存储方式，例如 canvas assets、wiki 原始文档、markdown、wiki pages。
5. 按全新项目设计，不考虑旧 JSON 目录、历史版本数据升级或旧包兼容。
6. 第一阶段不做细粒度 canvas record 表建模，先保存完整 panel state JSON，并抽出通用索引字段。

## 非目标

- 不在本阶段引入远程数据库或同步协议。
- 不把图片、PDF、音频、视频等二进制内容写入 SQLite BLOB。
- 不重写 studio 产品交互模型。
- 不一次性完成 canvas diff-log 或多用户协作。
- 不提供旧 JSON 数据导入、回滚或兼容层。

## 总体设计

新增 SQLite 数据库：

```text
.myopenpanels/
  main.sqlite3
  main.sqlite3-wal
  main.sqlite3-shm
  assets / raw / rules / wikis 等大文件目录继续保留
```

本地存储层从 `LocalOpenPanelsStorage` 升级为 SQLite-backed 实现。外部调用仍然使用：

```ts
readSession()
writeSession()
readPanel()
writePanel()
readPanelState()
writePanelState()
listArtifacts()
writeArtifact()
readPanelSelection()
writePanelSelection()
writeAssetFromFile()
writeAssetFromBuffer()
readAsset()
```

推荐实现方式：

- `crates/openpanels-local/src/storage.rs` 封装 SQLite connection、PRAGMA、migration、typed queries。
- Rust storage 默认使用 SQLite。
- 不保留 JSON file storage 作为运行时兼容层。
- 如果 `.myopenpanels/main.sqlite3` 不存在，直接创建全新数据库并执行 migrations。

## SQLite Driver

OpenPanels 的本地 CLI/server 已迁移到 Rust。当前 storage 实现使用 Rust `rusqlite`：

- 本地嵌入式 SQLite，事务 API 简单，适合当前 local server/CLI 的小型写入场景。
- 可以显式执行 `PRAGMA journal_mode = WAL`、`PRAGMA foreign_keys = ON`、`PRAGMA busy_timeout = 5000`。

## 数据库打开流程

伪代码：

```ts
export class LocalOpenPanelsStorage implements OpenPanelsStorage {
  constructor(options) {
    this.projectDir = resolve(options.projectDir)
    this.rootDir = resolve(options.storageDir ?? join(projectDir, ".myopenpanels"))
    assertSafeRoot(this.rootDir)
    this.dbPath = join(this.rootDir, "main.sqlite3")
    this.db = openDatabase(this.dbPath)
    migrate(this.db)
  }
}
```

`openDatabase()` 负责：

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;
```

`migrate()` 负责：

1. 确保 `schema_migrations` 表存在。
2. 按顺序执行未应用 migration。
3. 每个 migration 在事务里执行。
4. 成功后记录 migration id、checksum、applied_at。
5. 已应用 migration 启动时必须校验 checksum。

## Migration 机制

当前 Rust 实现在 `crates/openpanels-local/src/storage.rs` 内维护 migration registry。
每条 migration 包含：

- `id`
- `description`
- `checksum_material`
- `up(tx)`

Migration 表：

```sql
CREATE TABLE IF NOT EXISTS schema_migrations (
  id TEXT PRIMARY KEY NOT NULL,
  description TEXT NOT NULL,
  checksum TEXT NOT NULL,
  applied_at TEXT NOT NULL
);
```

规则：

- migration id 不允许修改或复用。
- 已发布 migration 不允许重写；需要新增 migration。
- migration 必须幂等到“只执行一次”的级别，即依赖 `schema_migrations` 控制，不依赖重复执行。
- 所有应用代码只支持迁移到最新 schema，不支持降级。
- 启动时如果发现未知 migration、非连续历史或 checksum mismatch，必须明确报错。
- `0001_initial` 保持不可变；通用 agent target、task delivery 和 lease token
  字段由 `0002_agent_task_dispatch` 增量升级。
- 测试必须覆盖空库初始化、migration 重入、checksum mismatch、未知 migration
  和失败回滚。

## 初始 Schema

### sessions

替代 `sessions/<sessionId>/session.json`。

```sql
CREATE TABLE sessions (
  id TEXT PRIMARY KEY NOT NULL,
  title TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  panel_ids_json TEXT NOT NULL DEFAULT '[]',
  session_json TEXT NOT NULL
);

CREATE INDEX sessions_updated_at_idx
  ON sessions(updated_at DESC, id ASC);
```

说明：

- `session_json` 保存完整协议对象，减少未来字段扩展成本。
- `title`、`updated_at` 抽出用于列表排序。
- `panel_ids_json` 第一阶段保留，方便维持当前 `OpenPanelsSession.panelIds` 数据形态；后续可完全由 `panels` 表派生。

### panels

替代 `panels/<panelId>/panel.json`。

```sql
CREATE TABLE panels (
  id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  title TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  state_ref TEXT,
  panel_json TEXT NOT NULL,
  PRIMARY KEY (session_id, id),
  FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX panels_session_kind_idx
  ON panels(session_id, kind, updated_at DESC);
```

### panel_states

替代 `state.json`。第一阶段继续保存完整 JSON。

```sql
CREATE TABLE panel_states (
  session_id TEXT NOT NULL,
  panel_id TEXT NOT NULL,
  schema_version INTEGER,
  state_json TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (session_id, panel_id),
  FOREIGN KEY (session_id, panel_id)
    REFERENCES panels(session_id, id)
    ON DELETE CASCADE
);
```

`schema_version` 从 `state.schemaVersion` 或 `state.schema.schemaVersion` 提取；提取不到则为 `NULL`。

### artifacts

替代 `artifacts.json`。

```sql
CREATE TABLE artifacts (
  id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  panel_id TEXT,
  kind TEXT NOT NULL,
  title TEXT,
  created_at TEXT NOT NULL,
  artifact_json TEXT NOT NULL,
  PRIMARY KEY (session_id, id),
  FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX artifacts_session_panel_idx
  ON artifacts(session_id, panel_id, created_at DESC);
```

### panel_selections

替代 `selection.json`。

```sql
CREATE TABLE panel_selections (
  session_id TEXT NOT NULL,
  panel_id TEXT NOT NULL,
  asset_ref TEXT,
  selected_shape_ids_json TEXT NOT NULL DEFAULT '[]',
  selection_json TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (session_id, panel_id),
  FOREIGN KEY (session_id, panel_id)
    REFERENCES panels(session_id, id)
    ON DELETE CASCADE
);
```

selection 截图仍然保存在 assets 目录，不写入 DB。

### wiki_tasks

Wiki task 当前存在 `state.tasks`，同时也会写入 `tasks/<taskId>.json`。SQLite 升级后任务应该可独立查询，避免每次读取整份 wiki state。

```sql
CREATE TABLE wiki_tasks (
  id TEXT PRIMARY KEY NOT NULL,
  session_id TEXT NOT NULL,
  panel_id TEXT NOT NULL,
  type TEXT NOT NULL,
  status TEXT NOT NULL,
  target_id TEXT NOT NULL,
  document_id TEXT,
  wiki_space_id TEXT,
  markdown_version INTEGER,
  claimed_by_process_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  task_json TEXT NOT NULL,
  FOREIGN KEY (session_id, panel_id)
    REFERENCES panels(session_id, id)
    ON DELETE CASCADE
);

CREATE INDEX wiki_tasks_status_idx
  ON wiki_tasks(status, updated_at ASC);

CREATE INDEX wiki_tasks_panel_status_idx
  ON wiki_tasks(session_id, panel_id, status, updated_at ASC);
```

第一阶段可以继续把 `tasks` 同步回 wiki `state_json`，保证 UI 和旧 helper 不需要一次性重写。第二阶段再让 `listWikiTasks()`、`nextWikiTask()`、`claimWikiTask()` 直接读写 `wiki_tasks`。

### key_values

用于未来存放 storage 级别元信息，也可承接部分 context metadata。

```sql
CREATE TABLE key_values (
  namespace TEXT NOT NULL,
  key TEXT NOT NULL,
  value_json TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (namespace, key)
);
```

## 文件系统边界

SQLite 只管理结构化数据和索引。以下内容仍然保存在文件系统：

- `panels/<panelId>/assets/**`
- wiki raw original files
- wiki generated/source markdown
- wiki rules markdown
- wiki pages markdown
- agent wakeup/run transient files
- studio logs

原因：

- 大文件放 SQLite 会让 DB 膨胀，备份和 WAL 行为更重。
- Markdown/wiki pages 需要保持 agent 可直接读写的文件形态。
- 当前大量 API 已经通过 `assetRef` / `wikiRef` 引用这些文件，保留能降低迁移风险。

SQLite 中只保存这些文件的 ref、metadata、索引字段和状态。

## Fresh Project Behavior

本项目按未上线的新项目处理，不实现旧文件格式升级：

1. 首次打开 storage 时创建 `.myopenpanels/main.sqlite3`。
2. 创建数据库后立即执行全部 migrations。
3. 新数据只写 SQLite；不再生成 `session.json`、`panel.json`、`state.json`、`artifacts.json`、`selection.json`。
4. 大文件目录仍按文件系统边界创建，例如 panel assets、wiki raw、wiki pages。
5. 如果开发环境里残留旧 JSON 目录，SQLite storage 不读取、不合并、不清理；需要人工删除或另写一次性脚本处理。

## Storage API 行为

### listSessions()

从 `sessions` 表按 `updated_at DESC` 读取，返回 `session_json` parse 后的对象。

### writeSession()

事务内 upsert：

1. 校验 `sessionSchema`。
2. 写 `sessions`。
3. 不写 `index.json`。

### readPanel() / writePanel()

从 `panels` 表读写 `panel_json`。写入时同步抽出 `kind`、`title`、`updated_at`。

### readPanelState() / writePanelState()

从 `panel_states` 表读写 `state_json`。

写入 wiki state 时：

- 抽取 `schemaVersion`。
- 如果 `state.tasks` 存在，同步 upsert 到 `wiki_tasks`。

写入 canvas state 时：

- 抽取 `state.schema.schemaVersion`。
- 暂不拆分 `store`。

### writeArtifact()

不再读取/重写整份 `artifacts.json`，改为单行 upsert/insert。

### writePanelSelection()

不再写 `selection.json`，改为 upsert `panel_selections`。selection asset 文件仍通过 `writeAssetFromBuffer()` 保存。

## Panel State Migration

数据库 schema migration 和 panel state migration 分开：

- 数据库 migration：表结构、索引、数据搬迁。
- panel state migration：`wiki`、`canvas` 等 panel 自己的 state schema 升级。

新增 panel state migrator：

```ts
export interface PanelStateMigration {
  panelKind: OpenPanelsPanelKind
  fromVersion: number
  toVersion: number
  migrate(state: unknown): unknown
}
```

读取 panel state 时：

1. parse JSON。
2. 根据 panel kind 调用 state migrator。
3. 如果 migrator 产生新 state，事务内写回 `panel_states`。

规则：

- `wiki schemaVersion: 1 -> 2` 必须有显式 migration，不能继续靠 `normalizeWikiState()` 直接丢弃旧结构。
- `canvas schema.schemaVersion` 也要进入迁移管线，即使当前只有 v1。
- 未识别的未来版本应该报错，而不是静默重置为空状态。

## Rust control 改造

`crates/openpanels-local/src/control.rs` 和 `crates/openpanels-local/src/wiki.rs`
里仍有文件路径协作逻辑。策略是“状态进 SQLite，大文件路径保留”：

- project/wiki bootstrap 继续通过 storage 读取 session、panel、state。
- `writeWikiMarkdown()`、`writeWikiPage()` 继续写 markdown 文件，但 metadata 和 task 状态通过 storage/DB 更新。
- `listWikiTasks()`、`nextWikiTask()`、`claimWikiTask()` 第二阶段改为直接查 `wiki_tasks`。
- `agent-targets.json`、`wakeups/`、`agent-runs/` 暂时保留在 `contexts/` 文件目录，因为它们是运行时协调信息，不是项目持久内容。

需要避免新增代码直接读取：

```text
sessions/<sessionId>/session.json
panels/<panelId>/panel.json
panels/<panelId>/state.json
artifacts.json
selection.json
```

这些路径不再作为项目状态的数据源。新代码只能通过 Rust storage API 读写结构化状态。

## API 和 CLI 合约

HTTP/CLI 输出字段尽量保持稳定，方便 agent 和 studio 调用：

- `openpanels-local panel list --format json`
- `openpanels-local panel current --format json`
- `openpanels-local panel switch --kind <kind> --format json`
- `openpanels-local wiki context --format json`
- `openpanels-local canvas state --format json`
- `openpanels-local canvas selection read --format json`
- wiki task/read/write commands

新增诊断命令建议：

```bash
openpanels-local storage status --project "$PWD" --format json
openpanels-local storage migrate --project "$PWD" --format json
```

`storage status` 输出：

```json
{
  "storageDir": ".../.myopenpanels",
  "databasePath": ".../.myopenpanels/main.sqlite3",
  "databaseExists": true,
  "latestMigrationId": "0002_agent_task_dispatch",
  "appliedMigrations": ["0001_initial", "0002_agent_task_dispatch"]
}
```

## 测试计划

### storage

1. 空项目初始化会创建 SQLite DB，并可创建 session/panel/state。
2. `listSessions()` 按 `updated_at DESC` 排序。
3. 新写入不会生成 `session.json`、`panel.json`、`state.json`。
4. `writeArtifact()` 不覆盖已有 artifact。
5. `deleteSession()` 级联删除 panels、states、artifacts、selections、wiki_tasks。
6. 非 `.myopenpanels` root 仍然拒绝。
7. 外部 `.myopenpanels` storageDir 仍然允许。

### migration

1. 空库执行全部 migrations。
2. 已执行 migration 不重复执行。
3. migration 中途失败会回滚。
4. checksum 或重复 id 异常能明确报错。
5. 未知未来 migration 和非连续 migration 历史会阻止启动。

### control / server

1. studio bootstrap 能从 SQLite 读取 project。
2. canvas 保存后重启 server 数据仍在。
3. wiki raw document 新增后 metadata 在 DB，original/markdown 文件在文件系统。
4. wiki task list/claim/complete 输出字段稳定。
5. selection asset 文件仍可读取。

## 分阶段落地

### Phase 1: SQLite Storage Foundation

- 引入 SQLite driver。
- 新增 SQLite open/migration 基础设施。
- 建立初始 schema。
- `LocalOpenPanelsStorage` 改为 DB-backed。
- 保持 assets 文件逻辑不变。
- 跑通现有测试并补 SQLite 初始化、migration、无 JSON shadow file 测试。

### Phase 2: Wiki Task Indexing

- `writePanelState()` 同步 wiki tasks 到 `wiki_tasks`。
- `listWikiTasks()`、`nextWikiTask()`、`claimWikiTask()` 读取 `wiki_tasks`。
- task complete/fail 同步更新 task row 和 wiki state。

### Phase 3: State Migration Discipline

- 新增 panel state migration registry。
- 实现 wiki v1/v2 migrator 测试。
- canvas state 进入 migration registry。
- 禁止未知未来版本静默 fallback。
- wiki/canvas state malformed 时明确报错，不能 normalize 成空 state。

### Phase 4: Maintenance Tools

- 增加 DB vacuum/backup 工具。
- 增加 `storage inspect` 或 `storage status`，方便排查 migration 和表统计。

## 风险和取舍

### 性能

SQLite 会解决列表查询、事务、频繁小对象更新和并发读写的一部分问题。但第一阶段仍然保存完整 `state_json`，所以超大 canvas 的整份序列化问题不会完全消失。

后续如果 canvas 规模明显变大，需要追加：

- `canvas_records` 表，按 record id 存储 shape/asset/page。
- `canvas_changes` append log。
- snapshot compaction。

### 数据版本

最大风险是 panel state migration。原则是：

- state 升级必须通过显式 migrator。
- 旧 state 不能被 normalize 成空状态；虽然当前不处理历史数据，但后续一旦发布版本就要遵守这个规则。
- 未知版本必须明确报错。

### 依赖

当前实现依赖 Rust `rusqlite`。storage 层隐藏具体 driver，后续如需替换 SQLite 绑定，不改变 CLI/API 合约。

## 已确认决策

1. SQLite DB 文件名使用 `main.sqlite3`。
2. 不考虑旧版本 JSON 数据升级和兼容；按全新项目实现。
3. 不需要 Phase 1 提供 JSON 导出/回滚能力。
4. wiki task 结构化状态以 DB 为准，不再写 `tasks/*.json` shadow copy。
