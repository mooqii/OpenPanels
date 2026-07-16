---
id: task-queue
title: MyOpenPanels Task Queue
description: Use when listing, claiming, executing, or completing work from the MyOpenPanels project Task queue.
source: builtin
appliesTo:
  - any
taskTypes:
requiresCommands:
  - task.list
  - task.next
  - task.read
  - task.claim-next
  - task.heartbeat
  - task.complete
  - task.fail
  - task.events
  - task.attempts
  - workflow.read
  - agent.target.register
  - agent.bridge.run
loadWhen:
  - When the request handles work from the generic project Task queue.
tokens: short
---

Use the generic Task commands returned by command catalog discovery. The `tasks`
table and generic Task service are authoritative; do not call queue-specific
HTTP endpoints or mutate panel state to change a Task lifecycle.

1. Register a target with every capability it can execute.
2. Claim work atomically with `task claim-next`.
3. Verify `executionProtocolVersion`, `executionGeneration`, and the immutable
   input manifest before performing task-bound writes.
   Protocol v3 additionally requires `taskBrokerUrl`, `executionToken`, and an
   Attempt staging session. Missing Broker configuration is fatal and must not
   fall back to direct file writes.
4. Route by `queue` and `capability`, then perform the requested panel writes.
5. Heartbeat long-running work before its lease expires. Stop immediately when
   heartbeat or a task-bound write returns `execution_fenced`.
6. Complete, fail, or release the Task with the returned lease token.

Use `task next` only for discovery. A runnable Task has `ready: true`; a Task can
be blocked by a live lease, a future retry time, exhausted attempts, or the lack
of a matching target. Poll targets actively claim work with
`task claim-next --wait-ms 25000`; they may be driven by an Agent message or a
persistent Worker. Command targets are local one-shot CLI processes managed by
`agent bridge run`. MyOpenPanels does not push Tasks to an Agent endpoint.

Never treat process exit zero as proof of success. `task complete` performs the
domain validation. A rejected completion is a failed Attempt; stop task-bound
writes as soon as the execution is fenced. Use `task events`, `task attempts`, and
`workflow read` when diagnosing prerequisite propagation or retries.
