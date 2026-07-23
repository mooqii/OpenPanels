# Skill Architecture

MyOpenPanels separates host integration from reusable content methods.

## Platform Skills

`myopenpanels-panels` and `myopenpanels-task-queue` are system contracts. The
first owns shared Canvas, Wiki, Writing, Typesetting, and Publishing mechanics;
the second owns generic
Task queue and handoff mechanics. They may describe MyOpenPanels context,
command discovery, target binding, storage operations, and Task or Operation
lifecycle. Agent Bootstrap loads the applicable system Skill before panel work.
Their packages live under `agent-resources/system-skills`.

## Portable Skills

Portable Skills contain only a standard `SKILL.md` package with `name` and
`description` frontmatter plus method instructions and optional bundled
resources. They define writing style, editorial structure, synthesis, or other
domain methods. They must not mention MyOpenPanels commands, Task ids, Agent
Bootstrap, Bridge lifecycle, or host storage.

Installed portable Skills are optional user content, not part of the CLI's core
feature contract. Their package shape is validated when loaded, but the CLI
does not negotiate an internal Skill schema version with them.

Built-in portable presets live under `agent-resources/preset-skills`. Both
system and preset packages use standard `name` and `description` frontmatter.
They are associated with panels, task types, and user-visible names through
`agent-resources/builtin-skill-registry.json`. That registry describes
Skill packages only; it does not own module capabilities.

`agent-resources/module-capability-catalog.json` is the platform-owned capability
registry. It uses one tagged collection for 19 stable Agent Procedures
and 9 Task Capabilities. Each capability declares its platform contract and
Local Skill policy. Procedure invocations add selection policy and minimum CLI
command intents; Task invocations own their exact routes, while Task Scope
invocations own supported scope kinds.
Procedure Bootstrap loads the thin Skill body, panel contract, and exact
function reference in that order. See `docs/module-capabilities.md` for the
current matrix and invariants.
Custom Skills use the same separation through one platform-owned manifest.
`manifest.json` owns the display name, canonical
`moduleKinds`, provenance, and installation metadata; `SKILL.md` remains a
portable package.

The independently installed Entry Skill contains a generated compact Procedure
and Task Capability intent index. System Panel Skills and their references ship inside
the CLI. Procedure Bootstrap resolves those synchronized packages and command
registrations at runtime, while unknown Entry Skill keys fall back to generic
Bootstrap.

Tasks are the only persisted execution entity. Dependencies, leases, results,
and up to three execution summaries live on the Task itself; orchestration and
Handler behavior ship with the current CLI release.

At execution time the host composes a Runtime Contract, a Task objective, the
selected portable Skill, and captured source material. The Runtime Contract is
authoritative for tools, reads, writes, targets, and lifecycle. The portable
Skill is authoritative only for the content method.

## Current contract

Custom Skill manifests are validated by their current shape rather than an
independent schema number. Platform Skill aliases and platform-specific
metadata inside `SKILL.md` are rejected. Task-created Skills are installed
from their committed immutable content revision into the same global Skill
package store used by imported Skills; this is a projection of a current Task
output, not a compatibility migration.

Agent Skill metadata exposed by the CLI, HTTP APIs, and Studio uses `name` for
the user-visible Skill name. The unrelated `title` fields used by Projects,
panels, and documents are unchanged.

Public marketplace discovery is intentionally deferred. A future importer
should preserve the external Skill package and create platform registration
data separately.
