# Agent Guidance Contract

This protocol implements the Procedure, Task, CLI authority, and Skill
boundaries defined in [`core-concepts.md`](core-concepts.md).

The CLI exposes three stable entries:

1. `agent bootstrap --procedure <key> --format json` resolves one static Agent
   Procedure against current Project state and returns only its context, Skill,
   reference, command descriptions, blockers, and state-bound actions.
2. `agent bootstrap --format json` remains the ambiguous-intent fallback for
   current user-visible context and discovery.
3. `agent catalog [--domain <domain>] --format json` returns the command domain
   index or one complete domain catalog.

Every response uses one CLI-owned envelope. The top-level shape always includes
`ok`, `intent`, `actions`, and `meta`, plus exactly one of `data` or
`error`. Actions are typed. CLI actions contain `intent`, `executor: "cli"`, and
an `argv` array; file reads, URL opens, and Skill installation use explicit action
kinds rather than shell command strings.

Execute `actions.required` in array order. Evaluate `actions.suggested` only
after all required actions succeed, using each action's structured condition.
Studio startup returns a required URL action followed by a conditional CLI
fallback action.

Bootstrap remains within 16384 UTF-8 bytes. Procedure Bootstrap distinguishes
the visible `focus` from its non-activating `target`, includes Procedure
metadata, and reports `readiness` plus structured `blockers`. It embeds the
owning System Skill body, exact reference bodies, required selection, target
versions, execution contract, and command descriptions for only the
Procedure's registered command intents. Generic Bootstrap retains progressive
domain catalog discovery. An unknown Procedure key returns generic Bootstrap
data with `procedureFallback` in the same response.

Task Capability keys are not Procedure aliases. Passing one to Procedure
Bootstrap follows the same unknown-key fallback as any other unregistered
Procedure. Task execution must preserve the exact claimed Task or
`task handoff start` contract. Tasks are the only persisted queued execution
entity. Procedure keys and Task Capability keys select code-owned behavior;
they are not stored workflow definitions.

The ExecutionBundle gives the Agent an artifact-only ExecutionResult contract.
The Agent writes declared workspace files; the shared Runtime Finalizer builds
a TaskOutputPlan, stages content, validates the execution fence, and completes
the Task. Agent-side Broker access is limited to Handler-declared reads and
Publishing checkpoints.
Only Registry-owned Task capabilities are advertised to Agent CLI workers; an
unregistered queue/type/capability tuple has no generic execution fallback.
Finalizer responses and development traces expose the `validating`, `applying`,
`committing`, `completed`, or `failed` phase without exposing credentials or
workspace paths in the persisted Task result.

Only the active selection is focus-bound. Other reads and writes target explicit
resources or panel kinds without requiring or changing the active panel.
