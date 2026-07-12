# Agent Guidance Protocol v4

## Purpose

MyOpenPanels exposes a small, pull-based Agent protocol. `agent bootstrap` gives
the Agent enough information to understand the current focus and choose what to
load next. Command parameters, Skills, Tasks, Operations, and selection
details are then read only when the user request needs them.

The separately installed `myopenpanels` entry Skill detects or installs the
native CLI, starts or reuses Studio, and follows the launch response's
`data.nextRequiredAction`. Bootstrap is requested only before panel work, not to
verify an open-only task.

Protocol v4 supersedes Protocol v3. Protocol and catalog versions are diagnostic
metadata, not inputs to Entry Skill behavior. The standard CLI envelope remains
at `schemaVersion: 1`.

## Communication Model

Communication remains local and pull-based:

```text
Agent -> current CLI -> SQLite/files
Studio -> local server -> SQLite/files
```

The CLI does not push prompts into the Agent. A normal discovery flow is:

1. Run `agent bootstrap --project-dir <project> --format json`.
2. If Bootstrap returns `update-entry-skill`, complete its one-time Agent-host
   update check and acknowledgement, then rerun Bootstrap before panel work.
3. Follow `data.nextRequiredAction.actionRefs` and execute every referenced Skill
   action sequentially in the listed order before evaluating any other action.
   This always includes the active Panel Skill and can also include the next Wiki
   Task's authoring Skill.
4. Read every returned local `SKILL.md` as instructed.
5. Choose any remaining applicable entries from `data.nextActions` according to
   `loadWhen`.
6. Execute their `argv` with the same resolved CLI executable and repeat against
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
- capability discovery commands, the required active Panel Skill reference,
  and the optional Task Queue Skill reference;
- a `load-required-skills` action that references every mandatory Skill load.

Normally Bootstrap contains no Entry Skill version check or update reminder. A
new CLI release latches its compiled Entry Skill requirement into the local
Agent control inbox. Until the current Agent context acknowledges that one-time
requirement, Bootstrap returns a compact `update-entry-skill` response instead
of Panel Skill and discovery actions. It contains one required Agent-host action
and one required CLI acknowledgement action. After acknowledgement, subsequent
Bootstraps return the normal payload without Entry Skill update fields.

Panel context is bounded to depth 4, strings of 256 UTF-8 bytes, arrays of 16
items, and objects of 32 fields. `contextTruncated` reports whether this happened.
Project, Panel, Task, and Operation identity fields are never shortened.

Bootstrap does not contain the full capability catalog or Skill content,
Task or Operation records, selection values, local paths, or Studio binding.
Because the Entry Skill requests Bootstrap only for panel-related work,
`activePanelSkill` is mandatory whenever Bootstrap is called. Its read action is
the first entry in `nextActions`, carries `required: true` and a stable
`actionRef`, and includes the project directory needed to execute it without
reconstructing context. When the active Wiki panel has a next ready Task,
Bootstrap also requires the authoring Skill captured by that Task and passes its
task id to the Skill loader. `nextRequiredAction.actionRefs` is the authoritative
ordered set of mandatory sequential loads; executable `argv` arrays remain
solely in `nextActions`.

## Progressive Discovery

Every Agent-facing response exposes follow-up CLI references only through the
top-level `nextActions` array. CLI actions use `executor: "cli"` semantics: their
`argv` excludes the executable, and the Agent prepends the exact CLI executable
it originally resolved. A required host step may be expressed directly by
`nextRequiredAction` with `executor: "agent-host"` and an instruction instead of
`argv`. Display `command` and `readCommand` strings are CLI-owned explanatory
data, not execution inputs.

An `agent skill read` response uses a required `agent-host` action to identify
the extracted local `SKILL.md`. Reading that file completes Panel Skill loading;
merely receiving the loader response does not.

`update install` may return an immediate advisory
`agent-host.skill.update-recommended` action when an Agent invoked the update.
That response is not relied upon for delivery. The replacement CLI records its
compiled Entry Skill requirement on the next Bootstrap, which also covers
Studio-initiated updates performed by an older CLI updater. Bootstrap emits the
required control actions only while the current context has not acknowledged
the requirement; it does not perform a network or version check on normal calls.

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
keeps schema version 2 and is deliberately independent from guidance Protocol v4.
Filesystem artifacts for completed or cancelled Operations are retained for seven
days, then removed during Studio housekeeping or later Operation activity. The
small database record remains available for history. Active and retryable failed
Operations are never pruned by this policy.

Target-bound writes never change the user's active Project, Panel, or selection.
Canvas completion still requires generation metadata. Wiki completion still
checks `contentVersion` and returns `content_conflict` rather than overwriting a
concurrent edit.

## Compatibility Policy

The permanent Agent work entry points are `studio start` and `agent bootstrap`.
Their core flags and envelope fields are compatibility contracts. All other
commands are discovered from returned actions and may evolve behind stable
capability intents.

The Entry Skill ignores `protocolVersion`, `commandCatalogVersion`, and
`cliVersion`. Internal protocol and command catalog revisions may advance with
the installed CLI, while the two entry commands, JSON envelope,
`nextRequiredAction`, and `CommandActionRef` remain stable.

User data, assets, Tasks, leases, deliveries, and Operations migrate forward
across released CLI versions. Business commands, flags, Command Catalog
projections, Guides, Panel Modules, internal Rust APIs, and Studio HTTP APIs only
support the currently installed package. Older CLIs must reject future data
schemas rather than downgrade or overwrite them.
