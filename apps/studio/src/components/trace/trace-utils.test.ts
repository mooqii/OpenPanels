import { describe, expect, it } from "vitest"
import type { ProjectTask, TraceCategory, TraceEvent } from "../../types"
import {
  canArchiveTask,
  formatTaskError,
  isPendingTask,
  manualAgentScopeCandidates,
  manualTaskInstruction,
  pendingTaskCount,
  retryTaskAgentMessage,
  traceEventMatchesFilter,
} from "./trace-utils"

function event(
  category: TraceCategory,
  summary: string,
  detail?: unknown
): TraceEvent {
  return {
    category,
    detail,
    id: `trace:${summary}`,
    seq: 1,
    summary,
    timestamp: "2026-07-17T00:00:00Z",
  }
}

describe("communication event filters", () => {
  it("hides the active-project heartbeat only from all", () => {
    const heartbeat = event("api", "GET /api/active-project -> 200", {
      method: "GET",
      path: "/api/active-project",
      status: 200,
    })

    expect(traceEventMatchesFilter(heartbeat, "all")).toBe(false)
    expect(traceEventMatchesFilter(heartbeat, "api")).toBe(true)
  })

  it("matches one event category at a time", () => {
    const cliEvent = event("cli", "myopenpanels task list")

    expect(traceEventMatchesFilter(cliEvent, "all")).toBe(true)
    expect(traceEventMatchesFilter(cliEvent, "cli")).toBe(true)
    expect(traceEventMatchesFilter(cliEvent, "agent")).toBe(false)
  })
})

function projectTask(overrides: Partial<ProjectTask>): ProjectTask {
  return {
    createdAt: "2026-07-17T00:00:00Z",
    id: "task:base",
    panelId: "panel:wiki",
    panelKind: "wiki",
    projectId: "project:test",
    queue: "wiki",
    status: "queued",
    targetId: "wiki:default",
    type: "ingest_markdown_into_wiki",
    updatedAt: "2026-07-17T00:00:00Z",
    ...overrides,
  }
}

describe("task archive rules", () => {
  it("allows terminal tasks and rejects queued or running tasks", () => {
    expect(canArchiveTask(projectTask({ status: "failed" }))).toBe(true)
    expect(canArchiveTask(projectTask({ status: "succeeded" }))).toBe(true)
    expect(canArchiveTask(projectTask({ status: "cancelled" }))).toBe(true)
    expect(canArchiveTask(projectTask({ status: "superseded" }))).toBe(true)
    expect(canArchiveTask(projectTask({ status: "queued" }))).toBe(false)
    expect(canArchiveTask(projectTask({ status: "running" }))).toBe(false)
    expect(isPendingTask(projectTask({ status: "queued" }))).toBe(true)
    expect(isPendingTask(projectTask({ status: "failed" }))).toBe(false)
  })

  it("uses the same pending definition for counts and filtering", () => {
    expect(
      pendingTaskCount([
        projectTask({ id: "task:queued", status: "queued" }),
        projectTask({ id: "task:failed", status: "failed" }),
      ])
    ).toBe(1)
  })
})
describe("manual task instructions", () => {
  it("includes the exact scope command in the current language", () => {
    const scope = { kind: "exact-task", taskId: "task:manual" } as const

    const english = manualTaskInstruction(scope, "en", {
      channel: "release",
    })
    const chinese = manualTaskInstruction(scope, "zh-CN", {
      channel: "release",
    })

    expect(english).toContain(
      "myopenpanels task handoff start --scope exact-task --task-id task:manual --format json"
    )
    expect(english).toContain("Do not claim or process another Task")
    expect(english).not.toContain("continue with each Bundle")
    expect(chinese).toContain("只处理任务 task:manual")
    expect(chinese).toContain("不要领取或处理其他任务")
    expect(chinese).not.toContain("Runtime 会返回下一项")
  })

  it("reserves project-wide continuation language for project drains", () => {
    const projectDrain = manualTaskInstruction(
      { kind: "project-drain", projectId: "project:test" },
      "en",
      { channel: "release" }
    )
    const wikiDrain = manualTaskInstruction(
      {
        kind: "wiki-mutation-drain",
        mutationKey: "wiki:project:panel:default",
        projectId: "project:test",
      },
      "en",
      { channel: "release" }
    )

    expect(projectDrain).toContain("continue with each Task returned")
    expect(wikiDrain).toContain("Do not process other Project tasks")
    expect(wikiDrain).not.toContain("continue with each Task returned")
  })

  it("uses one mutation scope without enumerating historical Task ids", () => {
    const instruction = manualTaskInstruction(
      {
        kind: "wiki-mutation-drain",
        mutationKey: "wiki:project:panel:default",
        projectId: "project:test",
      },
      "en",
      { channel: "release" }
    )

    expect(instruction).toContain("--scope wiki-mutation-drain")
    expect(instruction).toContain("--mutation-key wiki:project:panel:default")
    expect(instruction).not.toContain("--task-id")
  })

  it("keeps development and release Task Handoffs on separate CLIs", () => {
    const scope = { kind: "exact-task", taskId: "task:manual" } as const

    const development = manualTaskInstruction(scope, "zh-CN", {
      agentCli: "/checkout/scripts/myopenpanels-dev",
      channel: "development",
    })
    expect(development).toContain(
      "/checkout/scripts/myopenpanels-dev task handoff start"
    )
    expect(development).toContain("不要运行已安装的正式版 myopenpanels")

    const release = manualTaskInstruction(scope, "zh-CN", {
      channel: "release",
    })
    expect(release).toContain("myopenpanels task handoff start")
    expect(release).not.toContain("scripts/myopenpanels-dev task handoff start")
    expect(release).toContain("不要运行仓库内的 scripts/myopenpanels-dev")
  })

  it("folds conversion prerequisites into one ready Wiki mutation scope", () => {
    const conversion = projectTask({
      id: "task:convert",
      mutationKey: null,
      ready: true,
      type: "convert_document_to_markdown",
    })
    const ingest = projectTask({
      dependencies: [
        {
          failurePolicy: "cancel",
          prerequisiteTaskId: conversion.id,
          status: "queued",
          successCondition: "succeeded",
        },
      ],
      id: "task:ingest",
      mutationKey: "wiki:project:panel:default",
      ready: false,
      status: "queued",
    })

    expect(manualAgentScopeCandidates([conversion, ingest])).toEqual([
      {
        isReady: true,
        key: "wiki-mutation-drain:project:test:wiki:project:panel:default",
        scope: {
          kind: "wiki-mutation-drain",
          mutationKey: "wiki:project:panel:default",
          projectId: "project:test",
        },
      },
    ])
  })
})

describe("retry task Agent messages", () => {
  it("reads the failed task before retrying it exactly once", () => {
    const message = retryTaskAgentMessage("task:failed", "zh-CN", {
      channel: "development",
    })

    expect(message).toContain("scripts/myopenpanels-dev task read")
    expect(message).toContain(
      "scripts/myopenpanels-dev task retry --task-id task:failed --format json"
    )
    expect(message).toContain("只运行一次")
    expect(message).toContain("不要领取或执行新任务")
    expect(message).not.toContain("task handoff start")
  })
})

describe("task errors", () => {
  it("preserves Error messages", () => {
    expect(formatTaskError(new Error("Retry rejected"))).toBe("Retry rejected")
  })
})
