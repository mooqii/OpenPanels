import type { ProjectTask } from "../types"
import { BootstrapContractError } from "./bootstrap-contract"

export const TASK_STATUSES = [
  "queued",
  "running",
  "succeeded",
  "failed",
  "cancelled",
  "superseded",
] as const satisfies readonly ProjectTask["status"][]

export type TaskDisplayPhase =
  | "waiting"
  | "running"
  | "succeeded"
  | "failed"
  | "cancelled"

type TaskStatusValue = Pick<ProjectTask, "status">

const DISPLAY_PHASES: Record<ProjectTask["status"], TaskDisplayPhase> = {
  cancelled: "cancelled",
  failed: "failed",
  queued: "waiting",
  running: "running",
  succeeded: "succeeded",
  superseded: "cancelled",
}

export function taskDisplayPhase(task: TaskStatusValue): TaskDisplayPhase {
  return DISPLAY_PHASES[task.status]
}

export function isTaskStatus(status: string): status is ProjectTask["status"] {
  return (TASK_STATUSES as readonly string[]).includes(status)
}

export function assertCanonicalTaskStatuses(
  tasks: { id: string; status: string }[]
) {
  const invalid = tasks.find((task) => !isTaskStatus(task.status))
  if (invalid) {
    throw new BootstrapContractError(
      `Studio received unsupported Task status "${invalid.status}" for ${invalid.id}.`
    )
  }
}

export function taskIsActive(task: TaskStatusValue): boolean {
  return taskDisplayPhase(task) === "running"
}

export function taskIsSucceeded(task: TaskStatusValue): boolean {
  return taskDisplayPhase(task) === "succeeded"
}

export function taskIsTerminal(task: TaskStatusValue): boolean {
  return taskDisplayPhase(task) !== "waiting" && !taskIsActive(task)
}

export function taskCanCancel(task: TaskStatusValue): boolean {
  const phase = taskDisplayPhase(task)
  return phase === "waiting" || phase === "running"
}

export function taskCanRetry(task: TaskStatusValue): boolean {
  const phase = taskDisplayPhase(task)
  return phase === "failed" || phase === "cancelled"
}
