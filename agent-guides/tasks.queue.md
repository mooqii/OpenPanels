---
id: tasks.queue
title: Work With Project Tasks
source: builtin
appliesTo:
  - any
taskTypes:
requiresCapabilities:
  - tasks.list
  - tasks.next
  - tasks.inspect
  - agent.bridge.run
  - agent.bridge.status
loadWhen:
  - A panel exposes project tasks through the generic task queue.
tokens: short
---

Use this guide when working with the generic project task queue.

Task shape:

- `queue` identifies the producer, such as `wiki`.
- `capability` is the stable action name an agent should route on.
- `input` is the normalized task input.
- `source` points back to the originating panel object.
- `status` describes lifecycle state.
- `ready`, `blockedReason`, and `nextRunAt` say whether the task can run now.
- `attempt`, `maxAttempts`, `lease`, and `retryAfter` describe execution state.
- `result` and `error` describe the last outcome.

Workflow:

1. Read pending tasks with `openpanels-local tasks next --format json`.
2. Inspect the task when you need the full payload.
3. Route by `queue` and `capability`.
4. Use the queue-specific command set to claim, complete, or fail the task.
5. Keep the task id attached to any writes that complete the work.

Bridge mode:

- `openpanels-local agent bridge` runs the built-in task worker.
- `openpanels-local agent bridge status` reads the worker status.
- `openpanels-local agent bridge --command <command>` runs a local command for
  pending tasks instead of the built-in worker.
- The task JSON is sent to stdin.
- The command also receives `OPENPANELS_TASK_ID`, `OPENPANELS_TASK_QUEUE`, and
  `OPENPANELS_TASK_CAPABILITY`.
- Use `--timeout-ms <ms>` when the command can hang.
- The command is responsible for using the queue-specific claim, complete, and
  fail commands.
- The built-in worker uses queue-specific commands internally.

Scheduling:

- `tasks next` only returns a task when `ready` is true.
- A task can be blocked by a live lease, a future retry time, or exhausted
  attempts.

Do not invent a generic completion command. If a queue has no execution command
for a task, report that the task is visible but not executable by this agent.
