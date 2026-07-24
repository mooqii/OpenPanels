import { describe, expect, it } from "vitest"
import type { TaskStatus } from "../types"
import {
  taskCanCancel,
  taskCanRetry,
  taskDisplayPhase,
  taskIsActive,
  taskIsTerminal,
} from "./task-status"

const task = (status: TaskStatus) => ({ status })

describe("canonical Task status", () => {
  it("derives lifecycle behavior from the six persisted statuses", () => {
    expect(taskDisplayPhase(task("queued"))).toBe("waiting")
    expect(taskDisplayPhase(task("running"))).toBe("running")
    expect(taskDisplayPhase(task("superseded"))).toBe("cancelled")

    expect(taskIsActive(task("running"))).toBe(true)
    expect(taskIsActive(task("queued"))).toBe(false)
    expect(taskIsTerminal(task("succeeded"))).toBe(true)
    expect(taskIsTerminal(task("running"))).toBe(false)

    expect(taskCanCancel(task("queued"))).toBe(true)
    expect(taskCanCancel(task("running"))).toBe(true)
    expect(taskCanCancel(task("failed"))).toBe(false)
    expect(taskCanRetry(task("failed"))).toBe(true)
    expect(taskCanRetry(task("superseded"))).toBe(true)
    expect(taskCanRetry(task("succeeded"))).toBe(false)
  })
})
