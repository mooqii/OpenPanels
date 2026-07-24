# Module Capability Catalog

`agent-resources/module-capability-catalog.json` is the single registry for
Agent-facing module capabilities. Panel-bound selection procedures remain in
the same catalog because selection is genuinely panel state. It is independent from
`builtin-skill-registry.json`, which registers only installable System and
Preset Skill packages.

## Capability Model

Schema v2 stores every capability in one `capabilities` collection. Each entry
has a stable key, required owning `moduleKey`, optional panel kind, platform
contract, Local Skill policy, and a tagged invocation contract. `moduleKey`
owns capability classification; `panelKind` only declares panel context and
must not be used as a module or Task-queue classification.

- `procedure`: an Agent Procedure bootstrapped with
  `agent bootstrap --procedure <key>`. It declares a selection policy and CLI
  command intents.
- `task`: work executed through an exact Task. It declares one or more
  queue/type/capability/handler routes.
- `task-scope`: generic Studio handoff execution. It declares supported scope
  kinds instead of Task Handler routes.

The System Skill supplies the platform contract. `localSkill.mode` explicitly
selects `none`, `optional`, `required`, or `fixed`; a fixed policy also names
the Skill id. Current direct Procedures use `none`. Content-producing Task
Capabilities declare `required` where their captured input must contain a Local
Skill. Their `taskPointer` identifies that captured binding in the persisted
Task contract, so the shared constructor and storage boundary enforce the same
policy. Conversion and generic Task scopes use `none`.

Task routing reads queue, type, task capability, and Handler key only from this
catalog. The Rust Handler Registry owns executable functions and validation,
but no longer repeats route metadata. Domain task creators select a stable
Capability key and Task type; the shared Task constructor fills the route.
When a Task becomes executable, the Runtime resolves all System References from
the owning Capability and prepends them to its ExecutionBundle. Handlers no
longer select their own platform reference files.

## Current Matrix

<!-- BEGIN GENERATED CAPABILITY MATRIX -->
| Surface | Direct Procedures | Task Capabilities | Task Routes |
| --- | ---: | ---: | ---: |
| Canvas | 5 | 0 | 0 |
| Wiki | 4 | 2 | 3 |
| My Document | 4 | 0 | 0 |
| Writing | 1 | 1 | 1 |
| Typesetting | 1 | 3 | 3 |
| Publishing | 0 | 1 | 2 |
| Skills | 0 | 1 | 1 |
| Task queue | 4 | 1 | 0 |
| Total | 19 | 9 | 10 |
<!-- END GENERATED CAPABILITY MATRIX -->

All 19 direct Procedures are indexed by the MyOpenPanels Entry Skill and have
registered CLI command intents. All 9 Task Capability keys are indexed there as
non-Procedure routes. The ten concrete Task routes reference every static Task
Handler.

## Enforced Invariants

At CLI startup and in release checks:

- capability keys are globally unique;
- every capability references an existing Module Catalog key;
- the owning System Skill exists and supports the declared panel kind;
- every System Skill reference is relative, unique, and embedded in the package;
- every direct Procedure resolves to registered CLI command descriptors;
- the tagged invocation contract contains only fields valid for its kind;
- every Task route resolves to one Handler and every Handler is referenced;
- Task persistence rejects routes not registered by the catalog and validates
  required Local Skill bindings through the declared `taskPointer`;
- the generated Entry Skill index and documentation matrix match the catalog;
- Skill package registrations cannot contain legacy `procedures`,
  `taskHandoffs`, or `workflows` fields.

## Runtime Boundary

The catalog owns identity, routing, references, and Skill policy. CLI Command
Registry remains authoritative for command syntax. Task Handlers remain
authoritative for input materialization, dynamic instructions, output
validation, and finalization. Exact Procedure Bootstrap joins these sources
into one target-bound response containing the owning System Skill and
references, selection, resource versions, execution contract, and only its
registered command descriptors. No generic execution engine is introduced.
