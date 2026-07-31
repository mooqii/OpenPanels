import { describe, expect, it } from "vitest"
import type { ProjectTask } from "../types"
import {
  clearTasksIfNeeded,
  keepRequiredAgentMessageRequests,
  manualTaskInstructionAction,
  markManualAgentScopeObserved,
  unobservedReadyManualAgentScopes,
} from "./use-manual-task-instructions"

describe("manual task instruction queue", () => {
  it("preserves an empty queue reference to avoid a state update loop", () => {
    const tasks: ProjectTask[] = []

    expect(clearTasksIfNeeded(tasks)).toBe(tasks)
  })

  it("clears a populated queue", () => {
    const tasks = [{ id: "task:1" }] as ProjectTask[]

    expect(clearTasksIfNeeded(tasks)).toEqual([])
  })

  it("keeps Agent Message-only publishing instructions when an Agent CLI is usable", () => {
    const required = {
      requiresAgentMessage: true,
      scope: { kind: "exact-task" as const, taskId: "task:release" },
    }
    const optional = {
      requiresAgentMessage: false,
      scope: { kind: "exact-task" as const, taskId: "task:writing" },
    }

    expect(keepRequiredAgentMessageRequests([optional, required])).toEqual([
      required,
    ])
  })

  it("queues a newly created task only when no usable Agent CLI is available", () => {
    expect(manualTaskInstructionAction(false)).toBe("queue")
    expect(manualTaskInstructionAction(true)).toBe("ignore")
    expect(manualTaskInstructionAction(null)).toBe("await")
  })

  it("notices a task that becomes ready after first appearing blocked", () => {
    const scope = { kind: "exact-task" as const, taskId: "task:blocked" }
    const observedReadyKeys = new Set<string>()

    expect(
      unobservedReadyManualAgentScopes(
        [{ isReady: false, key: "exact-task:task:blocked", scope }],
        observedReadyKeys
      )
    ).toEqual([])
    expect(
      unobservedReadyManualAgentScopes(
        [{ isReady: true, key: "exact-task:task:blocked", scope }],
        observedReadyKeys
      )
    ).toEqual([scope])
  })

  it("does not rediscover a publishing task after its required instruction was shown", () => {
    const scope = { kind: "exact-task" as const, taskId: "task:release" }
    const observedReadyKeys = new Set<string>()

    markManualAgentScopeObserved(observedReadyKeys, scope)

    expect(
      unobservedReadyManualAgentScopes(
        [{ isReady: true, key: "exact-task:task:release", scope }],
        observedReadyKeys
      )
    ).toEqual([])
  })
})
