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

Protocol v3 supersedes Protocol v2 directly in unreleased CLI 0.4.0. There is no
v2 compatibility flag or full-Bootstrap mode. The standard CLI envelope remains
at `schemaVersion: 1`.

## Communication Model

Communication remains local and pull-based:

```text
Agent -> current CLI -> SQLite/files
Studio -> local server -> SQLite/files
```

The CLI does not push prompts into the Agent. A normal discovery flow is:

1. Run `agent bootstrap --format json`.
2. Match the user request to a scope in `data.discovery.recommendedScopes`.
3. Run `agent capability list --scope <scope> --format json`.
4. Read one full descriptor with `agent capability read --intent <intent> --format json`.
5. Read a referenced Skill or Guide only when its `loadWhen` rule applies.

## Bootstrap Contract

The successful JSON envelope, including its trailing newline, must not exceed
8192 UTF-8 bytes. The CLI serializes the final envelope before writing it. An
oversized result fails with `bootstrap_budget_exceeded`; it is never silently
truncated into an invalid target.

Bootstrap `data` contains only:

- protocol, catalog, CLI, budget, and Entry Skill versions;
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

`agent capability list --format json` returns a stable, sorted scope index.
`agent capability list --scope <scope> --format json` returns compact command
summaries. `agent capability read --intent <intent> --format json` returns the
full Command Registry descriptor, including Clap-derived arguments,
preconditions, target mode, output schema id, and large-output marker.

An unknown scope returns `capability_scope_not_found` with a recovery command
pointing back to the unfiltered scope index.

`agent guide list` and `agent skill list` accept optional `--panel-kind` and
`--task-type` filters. Lists contain metadata, `loadWhen`, and `readCommand` only.
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

The project has no public Protocol v1, Protocol v2, or pre-0.4 command
compatibility surface. The stable entry points are CLI installation, Studio
start/reuse, and compact `agent bootstrap`. All other Agent commands are
discovered progressively from the current CLI.

The CLI reports Entry Skill freshness metadata in Bootstrap but does not inspect
or update every Agent host's Skill installation itself.
