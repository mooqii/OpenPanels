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
  - tasks.claimNext
  - tasks.heartbeat
  - tasks.complete
  - tasks.fail
  - agent.targets.register
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

1. Register a target with the capabilities it can execute.
2. Claim work atomically with `tasks claim-next`.
3. Route by `queue` and `capability` and perform the requested panel writes.
4. Heartbeat long-running work before the lease expires.
5. Complete, fail, or release the task with the lease token returned by claim.

Studio mode:

- When Studio is started by a supported local agent host, it registers that
  host as a low-priority command target and starts processing tasks.
- Explicitly registered targets take priority over the automatic local target.
- Set `OPENPANELS_DISABLE_LOCAL_AGENT=1` to disable automatic local execution.
- Set `OPENPANELS_AGENT_COMMAND` to provide a host-neutral command target. It
  receives the standard task JSON and environment variables described below.

Bridge mode:

- `openpanels-local agent bridge --command <command> --capability <name>`
  registers a local command target and processes matching tasks.
- `openpanels-local agent bridge status` reads the worker status.
- The task JSON is sent to stdin.
- The command receives task, target, capability, and lease values through
  `OPENPANELS_TASK_*` and `OPENPANELS_TARGET_ID`.
- Use `--timeout-ms <ms>` when the command can hang.
- Exit code zero completes the task. A nonzero exit or timeout fails it and
  schedules a retry.
- Use `--manual-lifecycle` only for an existing command that already completes
  or fails tasks itself.

Target modes:

- Webhook targets receive a signed wake notification and then claim the task.
- Poll targets use `tasks claim-next --wait-ms 25000`.
- Command targets are managed by `agent bridge`.
- A target only receives capabilities declared during registration.

Scheduling:

- `tasks next` only returns a task when `ready` is true.
- A task can be blocked by a live lease, a future retry time, or exhausted
  attempts.

If no target supports a capability, the task remains queued with `noTarget`.
Do not claim capabilities the target cannot execute.
