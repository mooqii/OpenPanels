# MyOpenPanels Architecture

## Runtime boundaries

MyOpenPanels has five supported panels: Wiki, Writing, Canvas, Typesetting, and Publishing. Every Project exposes
them in that fixed order. Panel state and structured runtime data live in
SQLite. Agent-managed Markdown, Wiki pages, generated documents, and Writing
Skills use immutable content revisions backed by a content-addressed object
store; Canvas and other large assets keep their existing panel storage.

The Rust CLI and local Studio server are transports over the same domain
services. Transport modules parse and serialize data but do not own SQL.
Storage repositories own SQL and transaction boundaries. A business mutation
and its `change_scopes` revision must commit in the same transaction.

## Data ownership

- `projects`, `panels`, and `panel_states` own Project and Panel state.
- `tasks`, `task_attempts`, `task_events`, and `agent_targets` are the authority
  for Task lifecycle, leases, retries, assignment, results, and errors.
- Wiki panel state owns documents, spaces, rules, page indexes, and ingestion
  projections. It does not persist Agent process records.
- Custom Writing Skills are immutable revision resources in shared
  MyOpenPanels storage; each generated Skill has a task-bound manifest and a
  self-contained `SKILL.md`.
- Typesetting panel state owns publication projects, ordered cover references,
  and Tiptap JSON content. Imported images are copied into the Typesetting
  panel so they do not depend on the source Canvas asset lifecycle.
- Publishing panel state is reserved for the publishing process scaffold.
- `agent_operations` owns persistent Canvas and Wiki generation operations.
- `studio/instance.json` owns the storage-wide Studio process binding, while
  `studio/focus/` owns the single user-visible Project and Panel focus.
- Agent context files contain only Agent-private loader and lifecycle state;
  they never select or own a Studio process.

The baseline contains 28 tables including `schema_migrations`: nine core
storage tables, six Workflow Run/Task tables, three Agent tables, eight content and
staging tables, and two Model Gateway tables. Historical delivery, dispatch
outbox, and content-import checkpoint tables are not part of the baseline.

## Storage baseline

New storage starts from the single complete `0001_initial` migration. Before the
first release, schema changes are folded directly into this baseline rather than
kept as upgrade-only migrations. Any non-current database is rejected with
`incompatible_storage_baseline`; there is intentionally no pre-release data
upgrade path.

Release data lives in `~/.myopenpanels/`. The previous platform-specific
default directory is deliberately not imported or deleted. Self-update startup
arguments that point to that previous default are redirected to the new release
directory; other explicitly selected storage directories are opened normally
and must match the current baseline.

| Surface | Status | Authority |
| --- | --- | --- |
| `studio start`, `agent bootstrap` | Stable | Clap command and CLI envelope |
| Task lifecycle | Stable | `tasks` and `/api/tasks/*` |
| Workflow Run queries | Stable | `workflow_runs` and `/api/workflow-runs/*` |
| Agent target status | Stable | `agent_targets` and read-only `/api/agent/targets` |
| Agent guidance | Skill-only | `agent skill list/read` |
| Panel kinds | Wiki, Writing, Canvas, Typesetting, and Publishing | `panels.kind` constraint |
| CLI self-update | Release-critical | Previously installed CLI updater |
| Wiki Task/target routes | Removed | No compatibility handler |

## Transaction rules

Repository write methods begin the transaction, apply the domain mutation,
record the matching `change_scopes` revision, and commit once. Transport and
domain service code never records a second revision. A Task lease, target
heartbeat, or lifecycle transition is therefore visible together with
the revision that announces it.

Wiki document ingestion fields are projections of canonical Task state. They
may be updated by the Wiki adapter as part of Task completion, but they never
hydrate or replace rows in `tasks`. Context JSON is not a Task or Agent process
store.

## Atomic Workflow Runs

The current schema includes persisted Workflow Run DAGs, versioned inputs, Attempts,
append-only Events, fencing generations, and capability routes. Tasks and Agent
Targets use execution protocol v3 exclusively; omitted target versions default
to v3, while v1 and v2 are rejected.

Ready Tasks are selected from dependency state and `available_at`. Claiming a
Task creates exactly one Attempt and advances its execution generation in the
same transaction. Cancellation, prerequisite deletion, content supersession,
lease recovery, and Executor removal revoke the lease and advance the fencing
generation before another Attempt can begin. Completion records the result,
Attempt, Event, dependent activation, Workflow Run status, and `change_scopes`
notification together.

Task execution is command-target-only. The Studio Worker atomically claims work
before starting a one-shot local CLI or BYOK process; `agent bridge run` uses the
same command-target lifecycle. External Workers cannot register or pull Tasks.

### Task Handoffs And Execution Bundles

Task Scope is the non-persistent selector over existing Task, Workflow Run,
lease, and mutation records. `exact-task` claims only its selected Task.
`project-drain` keeps claiming independent work in one explicit Project until a
live empty check succeeds or only blockers remain.
`wiki-mutation-drain` follows one Project-local `mutation_key`, first claims the
non-terminal prerequisites needed to unlock its head, and then advances Wiki
updates in `mutation_sequence` order. Compatible Wiki updates may share a
consolidated execution window bounded by 256 KiB of Task metadata and source
inputs; windowing does not create a stored Task group or impose a file count
limit.

A Task Handoff is the Message-channel runtime over one Scope. `task handoff
start` registers and claims just in time, then returns ExecutionBundle v2 with
the selected Task Handler's objective, captured inputs and Skills, workspace
files, inlined System References, fully bound work-command parameters, Agent
command allowlist, and artifact output contract. Its Delivery Contract also
renders the exact heartbeat, completion, failure, and stop commands. `complete`
and `fail` advance the same Scope and return the next Bundle. One-shot targets
bind the explicit Project so a copied handoff never follows later Studio focus
changes.

The static Task Handler Registry owns document conversion, document generation,
Writing Skill refinement, Wiki authoring, Typesetting, and Publishing. The
automatic Agent CLI and Agent Message delivery adapters use the same Bundle
builder and Runtime Finalizer. The Agent writes ExecutionResult v2 workspace
artifacts; the Handler builds TaskOutputPlan v1, and the Finalizer creates or
resumes Operations, stages outputs, and commits the Task. Delivery owns only
startup, credentials, heartbeat, and scope continuation. Task Handoff keeps
credentials in a private transient control file and exposes only Handler-allowed
reads or Publishing checkpoints.

Agent targets advertise only the Task capabilities present in the Handler
Registry. A queue/type/capability tuple must match one registered Handler before
an ExecutionBundle can be built; there is no generic Agent execution fallback.
The Runtime Finalizer reports `validating`, `applying`, `committing`, and
`completed` phases in development traces. Failed results identify the phase
that failed, while successful Task results persist the final plan hash, Handler,
Operation ids, and artifact hashes without workspace paths.

## Model Gateway

The Model Gateway is the provider-neutral execution boundary between Project
Tasks and model runtimes. It owns persisted runtime selection, adapter
discovery, connection tests, and command construction; the Task queue continues
to own claiming, leases, retries, completion, and validation. The gateway
registers one stable Command Target for every enabled and available runtime
connection. The selected connection is the default first choice; the other
connections remain eligible as ordered fallbacks. Runtime setting changes
refresh the Target without changing its connection identity.

`model_gateway_connections` stores transport-neutral
connection profiles, provider-specific configuration, executable overrides,
model selection, and future BYOK credential references.
`model_gateway_config` stores the active Local CLI and BYOK connection pointers
plus the current execution mode.

Local CLI adapters are registered in the CLI runtime registry rather than in a
database migration. Each definition supplies version and authentication probes,
model discovery, an isolated smoke invocation, and Task command construction.
On startup the registry idempotently synchronizes built-in connection rows by
stable ID. New adapters are inserted, adapter metadata is upgraded, removed
adapters are disabled, and executable paths plus user configuration are
preserved. Reads first compare the registry fingerprint, so a steady-state Task
worker does not acquire a SQLite write lock.

This is also the database extension contract. Additional Agent CLIs use
`transport = 'local_cli'`; custom model endpoints use `transport = 'byok'` and
may have multiple connection profiles for the same provider. Secrets are not
stored in configuration JSON: `credential_ref` points to a secure credential
store. Provider-specific, non-queryable options belong in `config_json`.
Adding a provider or adapter therefore requires code and connection data, not a
schema migration. A migration is reserved for a new persistence relationship,
database constraint, or queryable/indexed concept.

Local CLI and BYOK configurations share this persistence boundary. The built-in
Local CLI registry supports Codex, Hermes, Claude Code, OpenCode, Gemini CLI,
GitHub Copilot CLI, Cursor Agent, Qwen Code, Kimi Code, and Kilo Code. Codex
models are discovered from its structured model catalog; Hermes models are
discovered through an ACP `initialize` and `session/new` handshake; OpenCode,
Cursor, and Kilo Code use their native model-list commands. Providers without a
stable model-list command expose a small fallback catalog and preserve the
CLI's configured default. Model and reasoning selections are stored per Local
CLI connection. Studio only shows reasoning controls when the adapter supports
a per-invocation setting; OpenCode variants are additionally scoped to the
selected model. Every adapter supports an isolated smoke request
before selection and one-shot execution for claimed Tasks. The BYOK branch is
part of the stored contract and Studio surface but is not an available
execution mode yet.

Studio reaches this boundary through `/api/model-gateway/settings`,
`/api/model-gateway/local-clis`, and
`/api/model-gateway/local-clis/test`. Provider-specific process details stay in
`model_gateway`; panel code and Task producers never build model commands.

Local CLI discovery and activation are separate. A scan reports whether an
adapter executable is currently available but does not mutate its persisted
`enabled` flag. Users explicitly activate channels and arrange them by
`priority`; only enabled, available channels become Agent Targets. The first
enabled channel is the default primary connection. Newly registered adapters
start disabled, so installing a CLI never silently adds execution capacity.

Task deletion is intentionally not supported. Terminal Tasks may be archived,
which hides them from default lists while retaining dependencies, Attempts,
Events, and provenance.

## Task Channel Dispatch

Tasks describe business intent and capabilities; they never contain provider
commands or API-specific configuration. Model Gateway connections are stable
execution channels, Agent Targets are their runnable Project-local instances,
and Task Attempts record the channel and an immutable executor snapshot.

Dispatch first applies protocol, capability, health, and concurrency filters,
then evaluates the capability route in order. Retryable channel and output
failures exclude that channel for the remainder of the current route round, so
the next eligible channel can claim the Task immediately. When every channel in
the route has been attempted, the normal Task backoff applies before a new
round. Terminal Task failures do not fall through to another channel.

Each Task has a dispatch mode. `auto` follows the route, while `prefer` tries a
requested Model Gateway connection before the remaining route. A preferred
channel is never an exclusive pin: retryable failure immediately falls through
to the next eligible channel. Channel selection and Attempt creation remain
inside the Task reservation transaction so concurrent Targets cannot claim the
same Task. The Task queue continues to own leases, retry budgets, fencing,
lifecycle transitions, and final validation; the gateway owns adapter execution
and normalized failure classification.

Studio exposes these controls at both levels: Model Gateway settings manage
channel activation and default order, while pending Task details can select
automatic or preferred-channel dispatch. Task overrides
reference stable Model Gateway connection IDs rather than ephemeral Agent
Target IDs.

## Atomic Content Broker

The initial schema includes content resources, immutable revisions, CAS
objects, Attempt staging sessions, and execution-token fencing. All Tasks use
execution protocol v3. The Agent-facing CLI forwards task-scoped reads and
writes to the Studio Task Broker; it never receives the SQLite path or global
storage write access.

Content access is manifest-based rather than capability-based. A leased
Attempt may read only resources recorded in its immutable `task_inputs` plus
the pinned base revision of its declared output, and may write only that
declared output through its staging session. Task capabilities select an Agent
route, while the Task type declares the output contract; capabilities do not
maintain a second resource-permission matrix. This keeps routing, input
capture, and content authorization from drifting apart.

Broker writes create invisible staging records. Task completion validates the
candidate manifest, then activates the content revision together with panel
state, Writing Operation state, Task/Attempt/Event state, dependent activation,
and change scopes in one SQLite transaction. CAS bytes are
written before that transaction but have no visibility without the active
revision pointer. Cancellation, lease loss, timeout, and generation changes
revoke the execution token and abandon staging. Old unpinned objects are
pruned asynchronously while revision metadata remains auditable.
