# Core Concepts and Authority Boundaries

## Purpose

This document defines the conceptual contract for the MyOpenPanels core. It is
the reference for future changes to Tasks, Operations, Procedures, CLI
commands, panels, module capabilities, and Skills.

The corresponding persistence contract for database ownership, revision
semantics, immutable content, migrations, and local file classes is defined in
[`storage-contract.md`](storage-contract.md).

Runtime code remains authoritative for current behavior while the repository is
being refactored. New implementation work should converge on the boundaries in
this document. Documentation must not be used to preserve an implementation
that violates these boundaries.

## Core principles

1. One persisted fact has one authoritative owner.
2. One business action has one write implementation.
3. A Task does one independently understandable unit of work.
4. Dependency ordering and mutation serialization are separate concepts.
5. CLI responses are the authoritative interface presented to an Agent.
6. Procedures and Skills reduce Agent discovery and round trips; they do not
   create a second implementation of platform behavior.
7. Panels own interaction state. Modules own business resources and behavior.

## Concept map

| Concept | Definition | Runtime instance | Authority |
| --- | --- | --- | --- |
| Module Capability | One atomic ability provided by a domain module | No | Rust domain implementation |
| CLI Command | A stable Agent-facing interface to an exposed capability | No | Clap definition and CLI Command Registry |
| Procedure | A predefined preparation recipe for one known user intent | No | Capability Catalog and Procedure resolver |
| Task | One queued, scheduled, leased, and retryable unit of work | Yes | SQLite `tasks` and Task Runtime |
| Operation | One direct, multi-command Agent interaction session | Yes | SQLite `direct_operations` and Direct Operation Runtime |
| Panel State | Focus, selection, draft, filter, and other interaction state | Yes | Panel and selection storage |
| Module Resource | A durable Document, Wiki Space, Canvas, Asset, Publication, Release, or similar business object | Yes | The owning domain table and content storage |
| Entry Skill | Host integration and fast Procedure routing guidance | Installed projection | Generated from the Capability Catalog |
| System Skill | Platform usage contract loaded for an Agent execution | CLI-owned package | Built-in System Skill package |
| Preset or Custom Skill | A content method such as writing, Wiki maintenance, layout, or publishing style | Selected or captured package | Skill package content |

## Tasks

A Task performs one unit of work. Examples include:

- write one My Document;
- maintain a Wiki for one captured update;
- generate one publication cover;
- generate one set of publication titles;
- format one publication;
- publish one captured release.

A Task owns queueing, scheduling, execution leases, fencing, retries, terminal
status, and execution summaries. These concerns must not be independently
implemented by a panel, resource, Operation, or Skill.

The canonical persisted Task statuses are:

```text
queued
running
succeeded
failed
cancelled
superseded
```

Terms such as `waiting`, `noTarget`, `manual`, `converting`, `ingesting`, and
`committing` are derived dispatch or display phases. They are not additional
Task statuses.

### Dependencies

An explicit dependency means that a Task requires the successful result of a
predecessor:

```text
Task B depends_on Task A
```

The current core needs only one direct predecessor per Task. It does not need a
persisted workflow graph or a generic workflow engine.

Dependencies are always within one Project and cannot form a cycle. If a
prerequisite fails, is cancelled, or is superseded, every queued or running
descendant is terminated in the same transaction and any active executor is
fenced from further writes or completion.

### Mutation serialization

Serialization prevents concurrent mutation but does not imply that one Task
semantically consumes another Task's result.

All Wiki mutation Tasks in a Project use:

```text
mutation_key = wiki:<project-id>
```

They may be created independently and remain independently inspectable, but
only one may be `running` at a time. Eligible mutations execute in creation
order. Wiki reads do not use this mutation key. A real semantic prerequisite
still uses `depends_on_task_id` in addition to the mutation key.

## Procedures

A Procedure is not a Task, Operation, or persisted workflow. It is a predefined
CLI-owned preparation recipe for a known user intent.

The purpose of:

```bash
myopenpanels agent bootstrap --procedure <procedure-key> --format json
```

is to return a complete, target-bound Agent execution package in one response.
That package should include:

- current Project and relevant Panel context;
- the explicit target and any active selection required by the intent;
- captured target revisions or versions;
- the required System Skill body and exact references;
- only the relevant CLI command descriptors;
- readiness and structured blockers;
- required and conditional actions;
- input, output, completion, and recovery contracts.

A successful Procedure Bootstrap should eliminate separate Catalog discovery
and avoid follow-up context reads before the Agent begins the requested work.
Later CLI calls may still be required to submit an artifact or complete a
direct Operation.

Procedure performance is a core product concern. Useful measures are:

- zero Catalog queries after an exact Procedure match;
- zero additional context reads before execution;
- one complete Bootstrap response for the prepared intent;
- only the minimum mutation or artifact-submission commands after Bootstrap.

## Module Capabilities and CLI commands

A Module Capability is an atomic business ability. A CLI Command is the
Agent-facing interface for a capability that the product chooses to expose.
Not every internal capability needs a CLI command.

Capabilities may be:

- internal to Studio or the Runtime;
- exposed as a direct Agent CLI command;
- executable only through a Task Handler.

Panel names belong in capability names only when the capability genuinely
concerns focus, selection, or panel interaction state. Shared resources use
module names. For example, reading a My Document is `my-document.read`, not a
Wiki-panel command, because My Documents are consumed by multiple panels.

The CLI response is authoritative for the Agent. In particular, its target,
revision, blockers, actions, `argv`, and lifecycle state override guidance in
any Skill or prose document.

## Operations

An Operation is a direct, multi-command Agent interaction session. It exists
when an Agent must bind a target, perform work outside the CLI, and later submit
an artifact or terminal result.

Examples include:

```text
begin Canvas image generation
-> create and bind a placeholder
-> Agent generates a bitmap
-> complete the Operation with the bitmap
```

```text
begin direct My Document creation or revision
-> bind the document and base content version
-> Agent produces Markdown
-> complete the Operation with conflict checking
```

An Operation has no queue, Task dependency, execution lease, retry budget, or
Worker scheduling. Its conceptual statuses are:

```text
active
completed
failed
cancelled
```

The Direct Operation record is the only lifecycle authority. A pending My
Document stores only the active `operationId` association and removes that
association when the Operation finishes; it does not persist a second copy of
the Operation status or error.

Begin atomically binds the Project, Panel, target, and base revision while
creating any placeholder or pending document. Completion revalidates those
bindings and commits the terminal Operation with its database mutation.
External bytes are prepared as immutable content first; a completed Operation
records the exact prepared revision so startup recovery has one durable source
for pointer publication.

Operations are for direct Procedures only. A Task Handler must not create a
nested Operation to represent Task output. A Task ExecutionBundle declares its
artifacts, and the Task Finalizer validates and commits them directly.

Thus:

- Procedure is a reusable definition;
- Operation is one direct run of a multi-command Procedure path;
- Task is one scheduled asynchronous work item.

## Panels and modules

A Panel is a user workspace, not a business ownership boundary. It owns:

- focus and ordering;
- explicit selection;
- drafts and filters;
- active resource choices;
- other presentation and interaction state.

A module owns stable resources and behavior. The same My Document can be used
by Wiki, Writing, and Typesetting without being copied into each panel. Task
status and module resources must not be persisted again inside panel UI state.
SQLite owns each resource's stable identity and active content revision;
immutable content owns the bytes of that revision. Filesystem pointers and
materialized views are projections, not additional owners.

## Skill layers

### Entry Skill

The Entry Skill lets the host Agent recognize MyOpenPanels requests and take
the fastest correct route. It may contain a compact Procedure index so an exact
user intent can go directly to Procedure Bootstrap.

The Procedure index must be generated from the Capability Catalog. The Entry
Skill must not hand-maintain command syntax, Task state rules, or detailed
platform behavior. It is a synchronized projection, not an independent source
of truth.

### System Skills

System Skills define platform usage contracts. Procedure Bootstrap and Task
ExecutionBundle assembly load the exact System Skill and references needed for
the current capability. The Agent should not need separate Skill discovery.

### Preset and Custom Skills

Preset and Custom Skills define content methods. They may control writing
style, Wiki organization, title strategy, layout choices, or publishing
method. They must not define MyOpenPanels CLI syntax, Task lifecycle,
Operation lifecycle, target binding, or storage behavior.

## Canonical execution paths

### Direct Procedure

```text
User request
-> Entry Skill selects an exact Procedure
-> Procedure Bootstrap returns the complete prepared context
-> Agent performs the content or tool work
-> Agent invokes the minimum direct CLI commands
-> optional Operation completion commits the result
```

### Task execution

```text
User or Studio creates one Task
-> scheduler applies dependency and mutation-key rules
-> Runtime atomically claims the Task
-> Handler returns an ExecutionBundle
-> Agent produces declared artifacts
-> Handler validates output and builds a TaskOutputPlan
-> Runtime atomically commits Task and domain mutations
-> prepared immutable content is published
```

## Authority hierarchy

Authority is assigned per fact rather than concentrated in one oversized
registry:

- Rust domain code owns behavior.
- The Capability Catalog owns Agent capability identity, Procedure membership,
  Task routes, System Skill references, and Local Skill policy.
- Clap owns CLI syntax; the Command Registry owns Agent-facing command
  metadata.
- SQLite Task rows own scheduled execution state.
- Domain tables and immutable content own module resources.
- Panel storage owns UI state and selection.
- Operation Runtime owns direct multi-command sessions.
- CLI structured responses are the final authority presented to an Agent.
- Entry and System Skills are generated or CLI-owned guidance that helps the
  Agent use those authorities efficiently.
