---
id: task-queue
title: MyOpenPanels Task Queue
description: Use when listing, claiming, executing, or completing work from the MyOpenPanels project Task queue.
source: builtin
appliesTo:
  - any
taskTypes:
requiresCapabilities:
  - task.list
  - task.next
  - task.read
  - task.claim-next
  - task.heartbeat
  - task.complete
  - task.fail
  - agent.target.register
  - agent.bridge.run
loadWhen:
  - When the request handles work from the generic project Task queue.
tokens: short
---

Use the generic Task commands returned by capability discovery. The `tasks`
table and generic Task service are authoritative; do not call queue-specific
HTTP endpoints or mutate panel state to change a Task lifecycle.

1. Register a target with every capability it can execute.
2. Claim work atomically with `task claim-next`.
3. Route by `queue` and `capability`, then perform the requested panel writes.
4. Heartbeat long-running work before its lease expires.
5. Complete, fail, or release the Task with the returned lease token.

Use `task next` only for discovery. A runnable Task has `ready: true`; a Task can
be blocked by a live lease, a future retry time, exhausted attempts, or the lack
of a matching target. Webhook targets receive signed wake notifications and
then claim work. Poll targets use `task claim-next --wait-ms 25000`. Command
targets are managed by `agent bridge run`.
