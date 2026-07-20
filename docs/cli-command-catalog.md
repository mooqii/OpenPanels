# CLI Command Catalog Contract

The Agent command catalog is the machine-readable authority for the business CLI.
Catalog v3 adds Task execution scope commands; the CLI envelope and Agent
Guidance Protocol versions are unchanged.
Run `myopenpanels agent catalog --format json` for the domain index, then run
`myopenpanels agent catalog --domain <domain> --format json` once to obtain every
command definition in that domain.

Each command contains only:

- `intent`: stable semantic identifier.
- `description`: concise purpose.
- `argv`: parseable example argument vector.
- `args`: typed argument definitions.
- `risk`: `read`, `write`, or `high-risk-write`; risk never adds confirmation.
- `target`: resource targeting and selection requirements.
- `retry`: structured retry policy.

Catalog domains contain only commands whose audience is `agent`. Host and
protocol commands are omitted. Worker/operator commands are isolated in the
`worker` domain. Every public Clap leaf has exactly one central command
definition, and every Agent command occurs in exactly one catalog domain.

Target modes distinguish `panel-kind`, `active-selection`, `task-bound`,
`task-scope`, and `operation-bound`. Panel reads and writes target their resource
without changing the user's focus. Only commands that inspect or export the
current selection use the active panel.
