# Agent Guidance Protocol v12

Protocol v12 uses three stable entries:

1. `agent bootstrap --procedure <key> --format json` resolves one static Agent
   Procedure against current Project state and returns only its context, Skill,
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

Bootstrap remains within 8192 UTF-8 bytes. Procedure Bootstrap distinguishes the
visible `focus` from its non-activating `target`, includes a
`procedureCatalogVersion`, and reports `readiness` plus structured `blockers`.
It loads only the owning Panel Skill and relevant reference, and embeds command
descriptions for the Procedure's registered command intents. Generic Bootstrap
retains progressive domain catalog discovery.

Task Handoff keys reject Procedure Bootstrap with `task_handoff_required` and
must preserve the exact claimed Task or `task handoff start` contract. Workflow
Runs are persisted, stateful Task DAG executions. They use `workflowRunId` and
`definitionKey` and are read through the Command Catalog v5 `workflow.run.*`
intents. Procedure keys and Task Handoff keys are not Workflow Run ids.

ExecutionBundle v2 gives the Agent an artifact-only ExecutionResult v2
contract. The Agent writes declared workspace files; the shared Runtime
Finalizer builds TaskOutputPlan v1, creates or resumes Operations, stages
content, validates the Attempt fence, and completes the Task. Agent-side Broker
access is limited to Handler-declared reads and Publishing checkpoints.
Only Registry-owned Task capabilities are advertised to Agent targets; an
unregistered queue/type/capability tuple has no generic execution fallback.
Finalizer responses and development traces expose the `validating`, `applying`,
`committing`, `completed`, or `failed` phase without exposing credentials or
workspace paths in the persisted Task result.

Only the active selection is focus-bound. Other reads and writes target explicit
resources or panel kinds without requiring or changing the active panel.
