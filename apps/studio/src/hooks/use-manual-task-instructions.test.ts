import { describe, expect, it } from "vitest"
import type { ProjectTask } from "../types"
import { clearTasksIfNeeded } from "./use-manual-task-instructions"

describe("clearTasksIfNeeded", () => {
  it("preserves an empty queue reference to avoid a state update loop", () => {
    const tasks: ProjectTask[] = []

    expect(clearTasksIfNeeded(tasks)).toBe(tasks)
  })

  it("clears a populated queue", () => {
    const tasks = [{ id: "task:1" }] as ProjectTask[]

    expect(clearTasksIfNeeded(tasks)).toEqual([])
  })
})
