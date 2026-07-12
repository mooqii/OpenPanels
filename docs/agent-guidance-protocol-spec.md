# Agent Guidance Protocol v3

## Purpose

MyOpenPanels exposes a small, pull-based Agent protocol. `agent bootstrap` gives
the Agent enough information to understand the current focus and choose what to
load next. Command parameters, Skills, Guides, Tasks, Operations, and selection
details are then read only when the user request needs them.

The separately installed `myopenpanels` entry Skill detects or installs the
native CLI, starts or reuses Studio, and follows the launch response's
`data.nextRequiredAction`. Bootstrap is requested only before panel work, not to
verify an open-only task.

Protocol v3 supersedes Protocol v2. Protocol and catalog versions are diagnostic
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
2. Follow `data.nextRequiredAction`.
3. Choose an applicable entry from `data.nextActions` according to `loadWhen`.
4. Execute its `argv` with the same resolved CLI executable.
5. Repeat against the next response until the user request is complete.

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
- capability discovery commands, the conditional active Panel Skill reference,
  and at most eight applicable Guide references;
- a `match-user-request` next action.

Panel context is bounded to depth 4, strings of 256 UTF-8 bytes, arrays of 16
items, and objects of 32 fields. `contextTruncated` reports whether this happened.
Project, Panel, Task, and Operation identity fields are never shortened.

Bootstrap does not contain the full capability catalog, Skill or Guide content,
Task or Operation records, selection values, local paths, or Studio binding.
`activePanelSkill` is a conditional reference, not an instruction to load it for
every request.

## Progressive Discovery

Every Agent-facing response exposes follow-up references only through the
top-level `nextActions` array. CLI actions use `executor: "cli"` semantics: their
`argv` excludes the executable, and the Agent prepends the exact CLI executable
it originally resolved. Advisory host actions use `executor: "agent-host"` and
an instruction instead of `argv`. Both forms carry a stable intent and a
`loadWhen` condition. Display `command` and `readCommand` strings are CLI-owned
explanatory data, not execution inputs.

`update install` is the only command that may return the advisory
`agent-host.skill.update-recommended` action. It asks the Agent to compare the
loaded Entry Skill metadata with the release-manifest version and consider an
update when older. Bootstrap never emits this reminder.

`agent capability list --format json` returns a stable, sorted scope index.
`agent capability list --scope <scope> --format json` returns compact command
summaries. `agent capability read --intent <intent> --format json` returns the
full Command Registry descriptor, including Clap-derived arguments,
preconditions, target mode, output schema id, and large-output marker.

An unknown scope returns `capability_scope_not_found` with a recovery command
pointing back to the unfiltered scope index.

`agent guide list` and `agent skill list` accept optional `--panel-kind` and
`--task-type` filters. Lists contain compact metadata, while their corresponding
read actions appear only in the response-level `nextActions` array.
Markdown, full capability requirements, and Skill local paths are returned by
the corresponding `read` command.

## Persistent Operations

Canvas image generation and Wiki document generation remain persistent
Operations. Begin captures the original target; read, complete, fail, and cancel
continue to work across Project or Panel switches and restarts. Operation storage
keeps schema version 2 and is deliberately independent from guidance Protocol v3.

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
