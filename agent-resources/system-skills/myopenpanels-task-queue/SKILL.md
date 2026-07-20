---
name: myopenpanels-task-queue
description: Use when listing, claiming, executing, or completing work from the MyOpenPanels project Task queue.
---

Use the generic Task commands returned by command catalog discovery. The `tasks`
table and generic Task service are authoritative; do not call queue-specific
HTTP endpoints or mutate panel state to change a Task lifecycle.

Intent routing:

- For read-only Task state, events, attempts, or Workflow inspection, read
  `references/inspect.md`.
- To retry a failed Task, read `references/retry.md`.
- To cancel a Task, read `references/cancel.md`.
- To archive a terminal Task, read `references/archive.md`.
- For an exact Studio Task scope handoff, read `references/execute-scope.md`.

1. Read the execution scope named by the manual handoff.
2. Execute the returned target registration action once. It binds a unique
   one-shot command target to the scope's explicit Project and capabilities.
3. Claim through `task scope claim` using the unchanged scope selector and the
   target id. Do not replace a scoped claim with queue-wide discovery.
4. Execute the claimed Task's newly returned required Skill actions.
5. Verify `executionProtocolVersion`, `executionGeneration`, and the immutable
   input manifest before performing task-bound writes.
   Protocol v3 additionally requires `taskBrokerUrl`, `executionToken`, and an
   Attempt staging session. Missing Broker configuration is fatal and must not
   fall back to direct file writes.
6. Route by `queue` and `capability`, then perform the requested panel writes.
7. Heartbeat long-running work before its lease expires. Stop immediately when
   heartbeat or a task-bound write returns `execution_fenced`.
8. Complete, fail, or release the Task with the returned lease token.
9. Repeat the same scoped claim after every finalized execution window. Exit
   only when `scopeState` is `complete` or `blocked`, summarize blockers when
   present, and execute the returned target removal action, including on failure.
   If it is `running` with no Task, wait for the reported lease or retry boundary
   before reading or claiming again; do not spin.

Use `task next` only for read-only discovery. A runnable Task has `ready: true`;
a Task can be blocked by a live lease, a future retry time, exhausted attempts,
or the lack of a matching command target. Persistent queue execution is owned by
the Studio Worker. Manual Agents process only the supplied execution scope.
`exact-task` never includes dependencies or Wiki batch members. `project-drain`
continues independent runnable Tasks before reporting blockers.
`wiki-mutation-drain` processes required prerequisites and then compatible Wiki
update windows in mutation order until the mutation queue is empty.

Never treat process exit zero as proof of success. `task complete` performs the
domain validation. A rejected completion is a failed Attempt; stop task-bound
writes as soon as the execution is fenced. Use `task events`, `task attempts`, and
`workflow read` when diagnosing prerequisite propagation or retries.
