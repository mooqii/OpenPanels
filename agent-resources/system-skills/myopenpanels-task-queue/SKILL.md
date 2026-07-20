---
name: myopenpanels-task-queue
description: Use when listing, claiming, executing, or completing work from the MyOpenPanels project Task queue.
---

Use the generic Task commands returned by command catalog discovery. The `tasks`
table and generic Task service are authoritative; do not call queue-specific
HTTP endpoints or mutate panel state to change a Task lifecycle.

Intent routing:

- For read-only Task state, events, attempts, or Workflow Run inspection, read
  `references/inspect.md`.
- To retry a failed Task, read `references/retry.md`.
- To cancel a Task, read `references/cancel.md`.
- To archive a terminal Task, read `references/archive.md`.
- For an exact Studio Task scope handoff, read `references/execute-scope.md`.

1. Run the exact `task handoff start` selector supplied by Studio. It registers
   and claims on execution, not when the instruction is copied.
2. Follow the returned ExecutionBundle and Delivery Contract without separately
   loading Catalogs, Panel Skills, or portable Skills.
3. Write only the declared workspace artifacts and `execution-result.json`.
   Use `task handoff exec` only for commands explicitly allowed by the Bundle;
   it injects the private protocol-v3 Broker and fencing context.
4. Heartbeat long work through `task handoff heartbeat`. Stop immediately when
   heartbeat or a work command returns `execution_fenced`.
5. Finish through `task handoff complete` or `task handoff fail`. The Runtime
   validates the result, creates any Operation, stages outputs, completes the
   Task, and immediately returns the next Bundle when the scope can continue.
6. Exit only at `complete` or `blocked`; use `task handoff stop` to abandon and
   release active work. Do not run low-level claim, complete, fail, or release
   commands for a handoff.

Use `task next` only for read-only discovery. A runnable Task has `ready: true`;
a Task can be blocked by a live lease, a future retry time, exhausted attempts,
or the lack of a matching command target. Persistent queue execution is owned by
the Studio Worker. Manual Agents process only the supplied execution scope.
`exact-task` never includes dependencies or Wiki batch members. `project-drain`
continues independent runnable Tasks before reporting blockers.
`wiki-mutation-drain` processes required prerequisites and then compatible Wiki
update windows in mutation order until the mutation queue is empty.

Never treat process exit zero as proof of success. `task handoff complete`
performs the Handler's domain validation. A rejected completion is a failed
Attempt; stop task-bound writes as soon as the execution is fenced. Use
`task events`, `task attempts`, and `workflow run read` when diagnosing
prerequisite propagation or retries.
