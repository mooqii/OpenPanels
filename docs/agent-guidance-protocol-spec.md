# Agent Guidance Protocol v9

Protocol v9 uses three stable entries:

1. `agent bootstrap --workflow <key> --format json` resolves one static Agent
   Workflow against current Project state and returns only its context, Skill,
   reference, command descriptions, blockers, and state-bound actions.
2. `agent bootstrap --format json` remains the compatibility and ambiguous-intent
   fallback for current user-visible context and discovery.
3. `agent catalog [--domain <domain>] --format json` returns the command domain
   index or one complete domain catalog.

Every response uses Envelope v3. The top-level shape always includes `ok`,
`schemaVersion`, `intent`, `actions`, and `meta`, plus exactly one of `data` or
`error`. Actions are typed. CLI actions contain `intent`, `executor: "cli"`, and
an `argv` array; file reads, URL opens, and Skill installation use explicit action
kinds rather than shell command strings.

Execute `actions.required` in array order. Evaluate `actions.suggested` only
after all required actions succeed, using each action's structured condition.
Studio startup returns a required URL action followed by a conditional CLI
fallback action.

Bootstrap remains within 8192 UTF-8 bytes. Workflow Bootstrap distinguishes the
visible `focus` from its non-activating `target`, includes a
`workflowCatalogVersion`, and reports `readiness` plus structured `blockers`.
It loads only the owning Panel Skill and relevant reference, and embeds command
descriptions for the Workflow's registered command intents. Generic Bootstrap
retains progressive domain catalog discovery.

Static Agent Workflow keys are not persisted Task `workflowId` values. A
handoff-only Workflow rejects Bootstrap and must preserve the exact claimed Task
or `task scope read` contract.

Only the active selection is focus-bound. Other reads and writes target explicit
resources or panel kinds without requiring or changing the active panel.
