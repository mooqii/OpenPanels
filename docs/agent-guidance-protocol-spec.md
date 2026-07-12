# Agent Guidance Protocol v5

## Purpose

MyOpenPanels exposes a small, pull-based Agent protocol. `agent bootstrap` gives
the Agent enough information to understand the current focus and choose what to
load next. Command parameters, Skills, Tasks, Operations, and selection
details are then read only when the user request needs them.

The separately installed `myopenpanels` entry Skill detects or installs the
native CLI, starts or reuses Studio, and follows the launch response's
`data.nextRequiredAction`. Bootstrap is requested only before panel work, not to
verify an open-only task.

Protocol v5 supersedes the pre-release Protocol v4. Protocol and catalog versions are diagnostic
metadata, not inputs to Entry Skill behavior. The standard CLI envelope remains
at `schemaVersion: 1`.

## Communication Model

Communication remains local and pull-based:

```text
Agent -> current CLI -> SQLite/files
Studio -> local server -> SQLite/files
```

The CLI does not push prompts into the Agent. A normal discovery flow is:

1. Run `agent bootstrap --format json`. Bootstrap resolves the running,
   user-visible Studio and does not depend on the Agent's working directory.
2. Complete every entry in `data.nextRequiredAction.steps` sequentially. A
   normal Bootstrap returns an Agent-host step containing the prepared required
   Skills. Read each `contextPath` first and `localPath` second.
3. If `nextRequiredAction.reason` is `update-entry-skill`, complete its one-time
   Agent-host update and CLI acknowledgement steps, then rerun Bootstrap.
4. Choose any remaining applicable entries from `data.nextActions` according to
   `loadWhen`.
5. Execute their `argv` with the same resolved CLI executable and repeat against
   each next response until the user request is complete.

## Bootstrap Contract

The successful JSON envelope, including its trailing newline, must not exceed
8192 UTF-8 bytes. The CLI serializes the final envelope before writing it. An
oversized result fails with `bootstrap_budget_exceeded`; it is never silently
truncated into an invalid target.

Bootstrap `data` contains only:

- diagnostic protocol, catalog, and CLI versions plus the Bootstrap budget;
- focus identities, `focusRevision`, and available Panel kinds;
- bounded Panel Module context and an explicit-selection summary;
- Task counts and at most one next-Task reference;
- active Operation count and at most three Operation references;
- optional capability, Task, Operation, and Skill discovery actions;
- a `complete-required-steps` action containing ordered local context and Skill
  paths for every mandatory Skill load.

Normally Bootstrap contains no Entry Skill version check or update reminder. A
new CLI release latches its compiled Entry Skill requirement into the local
Agent control inbox. Until the current Agent context acknowledges that one-time
requirement, Bootstrap returns `reason: update-entry-skill` instead of prepared
Panel Skills and discovery actions. Its required steps contain one Agent-host
action and one CLI acknowledgement action. After acknowledgement, subsequent
Bootstraps return the normal payload without Entry Skill update fields.

Panel context is bounded to depth 4, strings of 256 UTF-8 bytes, arrays of 16
items, and objects of 32 fields. `contextTruncated` reports whether this happened.
Project, Panel, Task, and Operation identity fields are never shortened.

Bootstrap does not contain the full capability catalog, Skill bodies, Task or
Operation records, selection assets, or Studio binding. It synchronizes built-in
Skills and writes bounded dynamic loader context under the current Studio
context directory. `nextRequiredAction.steps[].skills` contains the ordered
`contextPath` and `localPath` pairs. The active Panel Skill is always first; a
ready Wiki Task's captured authoring Skill follows when applicable. Capability
read actions required by those Skills are merged into `nextActions`.

## Progressive Discovery

Optional follow-up CLI references use the top-level `nextActions` array. CLI
actions use `executor: "cli"` semantics: their
`argv` excludes the executable, and the Agent prepends the exact CLI executable
it originally resolved. Required work appears only in
`nextRequiredAction.steps`; Agent-host steps contain instructions instead of
`argv`. Display command strings are CLI-owned explanatory data, not execution
inputs.

An `agent skill read` response uses a required `agent-host` action to identify
the extracted local `SKILL.md`. Reading that file completes Panel Skill loading;
merely receiving the loader response does not.

`update install` may return an immediate advisory
`agent-host.skill.update-recommended` action when an Agent invoked the update.
That response is not relied upon for delivery. The replacement CLI records its
compiled Entry Skill requirement on the next Bootstrap, which also covers
Studio-initiated updates performed by an older CLI updater. Bootstrap emits the
required control actions only while the current context has not acknowledged
the requirement. Normal calls only compare the compiled requirement with local
requirement and acknowledgement records; they perform no network or Agent-host
Skill version check.

`agent entry-skill acknowledge` records the installed version for the current
Agent context. It rejects stale event ids and versions below the current
requirement. Agent control records use global key-value storage and are not
Project Tasks, do not enter the task dispatcher, and do not bump panel state or
canvas snapshot revisions.

`agent capability list --format json` returns a stable, sorted scope index.
`agent capability list --scope <scope> --format json` returns compact command
summaries. `agent capability read --intent <intent> --format json` returns the
full Command Registry descriptor, including Clap-derived arguments,
preconditions, target mode, output schema id, and large-output marker.

An unknown scope returns `capability_scope_not_found` with a recovery command
pointing back to the unfiltered scope index.

`agent skill list` accepts optional `--panel-kind` and `--task-type` filters.
Lists contain compact metadata, while read actions appear only in the
response-level `nextActions` array. Markdown, full capability requirements, and
Skill local paths are returned by `agent skill read`.

## Persistent Operations

Canvas image generation and Wiki document generation remain persistent
Operations. Begin captures the original target; read, complete, fail, and cancel
continue to work across Project or Panel switches and restarts. Operation storage
keeps schema version 2 and is deliberately independent from guidance Protocol v5.
Filesystem artifacts for completed or cancelled Operations are retained for seven
days, then removed during Studio housekeeping or later Operation activity. The
small database record remains available for history. Active and retryable failed
Operations are never pruned by this policy.

Target-bound writes never change the user's active Project, Panel, or selection.
Canvas completion still requires generation metadata. Wiki completion still
checks `contentVersion` and returns `content_conflict` rather than overwriting a
concurrent edit.

## Development Policy

The project is pre-release. Protocol v5 is the only supported Agent guidance
shape; Protocol v4 `actionRefs` are intentionally not emitted. User data and
persistent Operations still migrate forward, but Agent protocol compatibility
is not maintained until the public compatibility policy is established.
