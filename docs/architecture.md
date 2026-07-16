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
- Publishing panel state is reserved for the publishing workflow scaffold.
- `agent_operations` owns persistent Canvas and Wiki generation operations.
- `studio/instance.json` owns the storage-wide Studio process binding, while
  `studio/focus/` owns the single user-visible Project and Panel focus.
- Agent context files contain only Agent-private loader and lifecycle state;
  they never select or own a Studio process.

## Compatibility

The permanent compatibility surface is `studio start`, `agent bootstrap`, the
CLI JSON envelope, released database migrations, and self-update behavior.
Business commands are discovered from the installed Command Registry. Studio
HTTP APIs support only the currently installed package; queue-specific legacy
Wiki Task routes are not supported.

Published migrations `0001` through `0004` are immutable. Later migrations
upgrade Session storage to Project storage, preserve Wiki and Canvas data, and
back up unsupported historical Panel records before removing them.

| Surface | Status | Authority |
| --- | --- | --- |
| `studio start`, `agent bootstrap` | Stable | Clap command and CLI envelope |
| Task lifecycle | Stable | `tasks` and `/api/tasks/*` |
| Agent targets | Stable | `agent_targets` and `/api/agent/targets/*` |
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

## Atomic Workflows

Migration `0011_atomic_workflows` keeps the public Task lease API while adding
Workflow DAGs, versioned inputs, Attempts, append-only Events, fencing
generations, and capability routes. Workflow-aware Tasks use
execution protocol v2, while atomic content Tasks use v3; legacy Targets remain
v1 until explicitly upgraded.

Ready Tasks are selected from dependency state and `available_at`. Claiming a
Task creates exactly one Attempt and advances its execution generation in the
same transaction. Cancellation, prerequisite deletion, content supersession,
lease recovery, and Executor removal revoke the lease and advance the fencing
generation before another Attempt can begin. Completion records the result,
Attempt, Event, dependent activation, Workflow status, and `change_scopes`
notification together.

Task transport is pull-based. Poll Targets actively call `claim-next`, while
Command Targets are local CLI bridges that claim work before starting a
one-shot process. Legacy Webhook Targets and delivery records remain in
upgraded databases only as inert historical data; they are hidden from routing
and public APIs.

## Model Gateway

The Model Gateway is the provider-neutral execution boundary between Project
Tasks and model runtimes. It owns persisted runtime selection, adapter
discovery, connection tests, and command construction; the Task queue continues
to own claiming, leases, retries, completion, and validation. The gateway
registers one stable Command Target for every enabled and available runtime
connection. The selected connection is the default first choice; the other
connections remain eligible as ordered fallbacks. Runtime setting changes
refresh the Target without changing its connection identity.

Migration `0014_model_gateway` promotes gateway state out of the generic
settings key/value store. `model_gateway_connections` stores transport-neutral
connection profiles, provider-specific configuration, executable overrides,
model selection, and future BYOK credential references.
`model_gateway_config` stores the active Local CLI and BYOK connection pointers
plus the current execution mode. The migration imports and removes the earlier
`settings/model_gateway` JSON row when present.

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

Local CLI and BYOK configurations share this persistence boundary. Phase one
implements `codex` and `hermes` Local CLI adapters. Codex models are discovered
from its structured model catalog; Hermes models are discovered through an ACP
`initialize` and `session/new` handshake. Both adapters support an isolated
smoke request before selection and use one-shot execution for claimed Tasks.
The BYOK branch is part of the stored contract and Studio surface but is not an
available execution mode yet.

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

Migration `0012_content_broker` adds content resources, immutable revisions,
CAS objects, Attempt staging sessions, and execution-token fencing. Content
Tasks require execution protocol v3. The Agent-facing CLI preserves its
commands but forwards task-scoped reads and writes to the Studio Task Broker;
it never receives the SQLite path or global storage write access.

Broker writes create invisible staging records. Task completion validates the
candidate manifest, then activates the content revision together with panel
state, Writing Operation state, Task/Attempt/Event state, dependent activation,
Outbox records, and change scopes in one SQLite transaction. CAS bytes are
written before that transaction but have no visibility without the active
revision pointer. Cancellation, lease loss, timeout, and generation changes
revoke the execution token and abandon staging. Old unpinned objects are
pruned asynchronously while revision metadata remains auditable.
