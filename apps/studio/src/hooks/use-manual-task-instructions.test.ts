import { describe, expect, it } from "vitest"
import type { ProjectTask } from "../types"
import {
  clearTasksIfNeeded,
  keepRequiredAgentMessageRequests,
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
})
