# Skill Architecture

MyOpenPanels separates host integration from reusable content methods.

## Platform Skills

`myopenpanels-panels` and `myopenpanels-task-queue` are system contracts. The
first owns shared Canvas, Wiki, and Writing mechanics; the second owns generic
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

Built-in portable presets live under `agent-resources/preset-skills`. Both
system and preset packages use standard `name` and `description` frontmatter.
They are associated with panels, task types, command intents, and user-visible
names through `agent-resources/builtin-skill-registry.json`. That platform
registry uses schema v4. Its `procedures` bind 18 stable Agent Procedure keys to
an ordered set of System Skill references, a selection policy, and the minimum
command intents required for that single capability. Procedure Bootstrap loads
the thin Skill body, panel contract, and exact function reference in that order.
Its five `taskHandoffs` bind direct Task execution routes without making them
bootstrappable Procedures. The registry is not part of any Skill package.
Custom Writing Skills use the same separation through their platform-owned
`manifest.json`.

The independently installed Entry Skill contains the compact Procedure and Task
Handoff intent index. System Panel Skills and their references ship inside the CLI. Procedure
Bootstrap resolves those synchronized packages and command registrations at
runtime, while unknown Entry Skill keys fall back to generic Bootstrap.

Workflow Runs are separate persisted executions made from Tasks, dependencies,
Attempts, and Events. Their `definitionKey` names the kind of full process and
leaves room for a future Workflow Definition Registry.

At execution time the host composes a Runtime Contract, a Task objective, the
selected portable Skill, and captured source material. The Runtime Contract is
authoritative for tools, reads, writes, targets, and lifecycle. The portable
Skill is authoritative only for the content method.

## Compatibility

Custom Writing Skill manifest schema v2 stores `name` and module binding outside
`SKILL.md`. Schema v1 packages remain readable and are converted to v2 only
after the user successfully saves their `SKILL.md`. Earlier schema v2 manifests
that use `title` instead of `name` are intentionally incompatible.

Agent Skill metadata exposed by the CLI, HTTP APIs, and Studio uses `name` for
the user-visible Skill name. The unrelated `title` fields used by Projects,
panels, and documents are unchanged.

Public marketplace discovery, remote installation, and compatibility selection
are intentionally deferred. A future importer should preserve the external
Skill package and create platform registration data separately.
