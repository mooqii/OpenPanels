# Core Consistency Optimization Plan

## Goal

Converge the current repository on the boundaries in
[`core-concepts.md`](core-concepts.md) without preserving unreleased legacy
contracts or introducing a generic workflow engine, broad compatibility layer,
or excessive runtime validation.

The result should provide:

- one authoritative owner for each persisted fact;
- one implementation for each business write;
- a small, stable Task kernel;
- serialized but independent Wiki mutation Tasks;
- direct Operations that never duplicate Task lifecycle;
- complete, low-round-trip Procedure Bootstrap responses;
- generated Entry Skill guidance that cannot drift from the Capability Catalog.

## Constraints

- The product has not been released. Old Procedure keys, Task statuses,
  storage shapes, and API aliases do not require compatibility.
- Runtime code is authoritative for current behavior during the refactor.
- Existing useful domain implementations should be retained rather than
  replaced with a generic abstraction.
- Validation belongs at real ownership boundaries. Do not add independent
  protocol versions or repeat the same invariant in every layer.
- Keep the current single-predecessor Task dependency model.
- Keep immutable content staging and recovery rather than attempting a
  distributed transaction between SQLite and the filesystem.

## Remaining gaps

The current repository already has a central Capability Catalog, CLI Command
Registry, Task Handler Registry, immutable content staging, and shared Task
Finalizer. Task lifecycle has been normalized, Task/domain completion now uses
one revision-checked transaction, and Writing Task output is now finalized
directly from its declared artifact without a nested Operation. The remaining
inconsistencies are:

1. Wiki authoring paths outside Task finalization still hydrate Tasks into a
   panel-shaped structure and can upsert Tasks and resources separately.

## Work package 0: restore a trusted baseline

Before architectural edits:

1. Produce a clean Rust build through the checkout-local development wrapper.
2. Run the complete Rust and Studio test suites, fixing any baseline failure
   before architectural changes begin.
3. Add focused characterization tests around:
   - concurrent Task claim;
   - Task cancellation;
   - Wiki mutation ordering;
   - Writing Task completion;
   - direct Canvas and My Document Operations;
   - Skill file editing through both current HTTP surfaces.
4. Record the expected current results without preserving legacy state names or
   API aliases as requirements.

Exit criteria:

- the repository builds;
- all tests pass;
- each high-risk flow has a test that observes its durable result.

## Work package 1: establish the Task kernel

Make claim, heartbeat, fail, release, retry, cancel, and archive generic Task
Runtime operations.

### Changes

1. Remove queue-specific calls from generic claim and heartbeat.
2. Atomically select and claim a Task inside one `IMMEDIATE` transaction.
3. Keep the canonical six Task statuses only.
4. Remove compatibility normalization for `reserved`, `claimed`,
   `converting`, `indexing`, `cancel_requested`, `stale`, and `blocked`.
5. Keep dispatch facts such as dependency waiting, mutation blocking,
   no-target, and manual execution as derived response fields.
6. Make retry create a linked Task and never reactivate a terminal Task.

### Wiki serialization

Assign every Wiki mutation Task in one Project:

```text
mutation_key = wiki:<project-id>
```

The scheduler must claim only the oldest eligible queued Task for that key.
Wiki reads do not use the key. Use `depends_on_task_id` only when a Task
semantically requires a predecessor's successful result.

Exit criteria:

- two concurrent claimers cannot both receive the same Task;
- only one Wiki mutation Task per Project can be running;
- unrelated Task queues can still execute concurrently;
- no panel or domain adapter owns Task lease state.

## Work package 2: make Task completion plan-driven

Keep Handler-specific validation and domain behavior, but centralize writes.

The current foundation uses `TaskOutputPlan` with a revision-bound prepared
panel state. The Task Runtime commits that state through the relational
resource decomposition layer in the same transaction as the Task transition.
If the captured resource revision changed, the entire transaction fails with a
content conflict and the Task remains running.

Introduce one result structure with only the data needed by the Runtime:

```rust
struct TaskOutputPlan {
    result: serde_json::Value,
    content_commits: Vec<ContentCommit>,
    resource_mutations: Vec<ResourceMutation>,
    panel_ui_mutations: Vec<PanelUiMutation>,
}
```

The exact Rust representation may use existing concrete types where available;
do not turn it into a configurable workflow language.

### Changes

1. Handler materialization prepares an ExecutionBundle.
2. Handler validation converts artifacts into validated domain values.
3. Handler finalization builds a `TaskOutputPlan` without writing Task state.
4. `finalize_task_runtime` commits the Task transition, resource mutations,
   Task-resource links, and change scopes in one SQLite transaction.
5. Immutable files remain staged before the transaction and published after
   commit through the existing recoverable content mechanism.
6. Remove Task-derived conversion and ingestion state from writable panel JSON.

Exit criteria:

- Task and domain-resource results cannot commit independently;
- Task completion has one write path;
- failure before commit changes neither Task nor resource state;
- recovery after database commit can finish content publication.

## Work package 3: remove Task-bound Operations

Operations remain part of direct Agent interaction, but no longer represent
Task output.

### Changes

1. Writing ExecutionBundles declare the My Document artifact and metadata
   directly.
2. Writing output validation checks the declared artifact.
3. The Task Finalizer creates or revises the My Document from the validated
   output plan.
4. Remove Writing lifecycle scans of Operations by `writingTaskId`.
5. Remove Task-owned Operation creation, preparation, and completion paths from
   the Broker and Finalizer.
6. Keep direct Canvas image and direct My Document Operation flows.

Exit criteria:

- a Writing Task can succeed without creating an Operation;
- no Operation contains a Task lifecycle status;
- cancelling a Task does not require separately cancelling an Operation;
- direct Procedures continue to support multi-command artifact submission.

## Work package 4: strengthen Procedure Bootstrap

Procedure performance is a core product feature, not merely documentation.
The current implementation returns the complete System Skill and references,
selection materialization, target versions, execution contract, and scoped
command descriptors in the exact Procedure response.

### Required Procedure response

Each exact Procedure Bootstrap should return:

- focus and explicit non-activating target;
- current target revision or version;
- required selection materialization;
- owning System Skill body;
- exact System Skill references;
- only the command descriptors needed by the Procedure;
- readiness, blockers, required actions, and conditional actions;
- artifact and completion contract where applicable.

### Changes

1. Add a `moduleKey` or equivalent explicit surface to every capability.
2. Stop using `panelKind: null` as a Task-queue display classification.
3. Generate the Entry Skill Procedure index exclusively from the Capability
   Catalog.
4. Keep descriptions and stable Procedure keys in the Entry Skill, but never
   copy command syntax or lifecycle rules into it.
5. Package the Entry Skill projection with the same build input as the CLI.
6. Make a missing exact Procedure fall back to generic Bootstrap, not to a
   legacy alias.

Exit criteria:

- an exact Procedure requires no follow-up Catalog call;
- all Procedure command descriptors parse through the current CLI;
- changing the Capability Catalog regenerates the Entry Skill and docs;
- module Procedures are grouped by module rather than by panel nullability.

## Work package 5: unify Skill management

Use one Skill package service for System, Preset, and Custom Skill reads, and
for allowed Custom Skill mutations.

The current implementation now exposes one generic Skill HTTP surface for
listing, reading, editing, and deleting packages. Writing consumes the same
module-associated representation as Settings and no longer owns parallel file
routes. Custom package edits and manifest changes use atomic file replacement,
portable content is rejected when it contains platform lifecycle instructions,
and deletion clears selections according to the package's declared module
associations.

### Changes

1. Move file traversal, path validation, portable Skill validation, atomic
   writes, deletion, and selection cleanup behind one service.
2. Query Skills by module association rather than by a separate implementation.
3. Migrate Writing UI calls to the generic Skill API.
4. Delete Writing-specific Skill file read, write, and delete routes.
5. Keep System and Preset Skills read-only.
6. Keep platform lifecycle text out of Preset and Custom Skills.

Exit criteria:

- one function owns Skill file writes;
- every editable Skill uses atomic replacement;
- Writing and Settings observe the same Skill representation and errors;
- deleting a Skill clears all module selections through one implementation.

## Work package 6: normalize Studio projections

The Studio consumes authoritative Task and resource responses instead of
reconstructing multiple lifecycle models.

The current implementation now rejects non-canonical Task statuses and
malformed panel responses at the bootstrap boundary. Task lifecycle display
and actions use the shared Task status projection, Wiki conversion and
ingestion status is projected by the storage layer from canonical Tasks and
durable ingestion records, and normalized Studio snapshots keep panel UI state
separate from hydrated module resources.

### Changes

1. Define the six canonical Task statuses as a strict TypeScript union.
2. Centralize `taskDisplayPhase`, `taskIsActive`, `taskCanCancel`, and
   `taskCanRetry`.
3. Remove independent status sets from Writing, Typesetting, Publishing, and
   Trace.
4. Derive Wiki conversion and ingestion display from Task status plus
   `wiki_source_ingestions`.
5. Do not silently replace a malformed server state with an empty business
   state. Surface a bootstrap or reload error instead.
6. Keep panel UI state separate from hydrated module resources in client update
   logic.

Exit criteria:

- the same Task has the same lifecycle interpretation in every panel;
- persisted business resources are not reconstructed from panel UI state;
- a contract mismatch is visible rather than presented as empty user data.

## Work package 7: make direct Operations internally consistent

Do this after Task-bound Operations are removed, so only the direct use case
remains.

The current implementation now stores Direct Operations in the clean SQLite
baseline with only `active`, `completed`, `failed`, and `cancelled` states.
Canvas placeholders and pending My Documents are created in the same
transaction as their active Operation. Completion revalidates the captured
target and revision; the terminal Operation, Canvas or document projection,
and relational asset mutation commit together. My Document bytes use immutable
content preparation, and completed Direct Operations participate in the same
startup pointer recovery as completed Tasks. Recovery also removes artifact
directories that have no Direct Operation record and expires old terminal
artifacts.

### Changes

1. Rename the internal type and module boundary to `DirectOperation` where this
   improves clarity; public command wording may remain `operation`.
2. Keep only the four direct statuses.
3. Ensure begin binds the exact Project, Panel, target, and base revision.
4. Ensure complete revalidates the bound target and revision.
5. Store the direct Operation record and its corresponding database mutation
   atomically. Because the product is unreleased, the clean schema baseline may
   add a small `direct_operations` table instead of retaining filesystem JSON
   records.
6. Continue using content staging for external artifact bytes.
7. Remove abandoned direct-operation artifact directories during recovery.

Exit criteria:

- a placeholder or pending document cannot exist without its active direct
  Operation;
- a completed direct Operation cannot point to an uncommitted resource result;
- restart recovery has one source for direct Operation status.

## Work package 8: remove unreleased compatibility code

After the new paths are tested:

1. Remove old Procedure keys and names.
2. Remove old Task status strings and normalizers.
3. Remove old Writing Skill HTTP aliases.
4. Remove Task-bound Operation fields and Broker commands.
5. Remove obsolete storage import paths and recreate the development storage
   from the clean baseline.
6. Update architecture and protocol documentation from the resulting code.
7. Delete tests that assert removed compatibility behavior; do not keep dead
   adapters solely to satisfy old fixtures.

The current implementation has completed this cleanup. The Capability Catalog
registers Procedure keys and Task Capability keys as distinct invocation
surfaces; a Task key passed as `--procedure` is simply an unknown Procedure.
Direct Operation lifecycle exists only in `direct_operations`, while a pending
My Document carries only its active `operationId` association. Task execution
accepts the workspace artifact contract directly, without legacy staged-result
test adapters or Task-bound Broker Operation routes. Rust and Studio consume
the canonical Task statuses, storage projection reads fields by explicit
resource kind, and tests for removed aliases and routes have been deleted.

## Minimal consistency checks

Keep checks focused on facts that cross ownership boundaries:

- every Capability key is unique;
- every Procedure command intent exists in the CLI Command Registry;
- every Task route references exactly one Handler;
- every Handler is referenced by a Task route;
- generated Entry Skill and capability documentation match the Catalog;
- built-in Skill registrations match actual packages;
- canonical Task status strings are not redefined by domain modules.

Do not introduce independent protocol versions, a generic workflow schema, or
runtime validation that repeats Rust type and database constraints.

## Implementation order

Implement and review the work in this order:

1. trusted build and characterization tests;
2. generic Task lifecycle and Wiki serialization;
3. plan-driven Task completion;
4. removal of Task-bound Operations;
5. complete Procedure Bootstrap and generated Entry Skill routing;
6. unified Skill management;
7. Studio status and projection cleanup;
8. direct Operation storage consistency;
9. deletion of unreleased compatibility code and final documentation update.

Each work package should land with its own tests and leave the repository
buildable. Avoid a single branch that changes Task Runtime, Procedures, Skills,
Studio state, and Operation persistence at the same time.
