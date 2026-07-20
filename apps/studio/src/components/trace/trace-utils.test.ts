import { describe, expect, it } from "vitest"
import type { ProjectTask, TraceCategory, TraceEvent } from "../../types"
import {
  groupWikiUpdateTasks,
  manualAgentScopeCandidates,
  manualTaskInstruction,
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

describe("Wiki update task groups", () => {
  it("projects serial Wiki mutations as one aggregate task", () => {
    const grouped = groupWikiUpdateTasks([
      projectTask({
        id: "task:one",
        mutationKey: "wiki:project:panel:default",
        mutationSequence: 1,
        status: "succeeded",
      }),
      projectTask({
        id: "task:two",
        mutationKey: "wiki:project:panel:default",
        mutationSequence: 2,
        status: "claimed",
        type: "maintain_wiki",
      }),
      projectTask({
        id: "task:conversion",
        mutationKey: null,
        type: "convert_document_to_markdown",
      }),
    ])

    expect(grouped).toHaveLength(2)
    const wikiGroup = grouped.find((task) => task.wikiUpdateGroup)
    expect(wikiGroup?.status).toBe("running")
    expect(wikiGroup?.wikiUpdateGroup?.taskIds).toEqual([
      "task:one",
      "task:two",
    ])
    expect(wikiGroup?.result).toEqual({ completedTaskCount: 1, taskCount: 2 })
  })
})

describe("manual task instructions", () => {
  it("includes the exact scope command in the current language", () => {
    const scope = { kind: "exact-task", taskId: "task:manual" } as const

    expect(manualTaskInstruction(scope, "en")).toContain(
      "myopenpanels task scope read --scope exact-task --task-id task:manual --format json"
    )
    expect(manualTaskInstruction(scope, "zh-CN")).toContain(
      "只处理任务 task:manual"
    )
  })

  it("uses one mutation scope without enumerating historical Task ids", () => {
    const instruction = manualTaskInstruction(
      {
        kind: "wiki-mutation-drain",
        mutationKey: "wiki:project:panel:default",
        projectId: "project:test",
      },
      "en"
    )

    expect(instruction).toContain("--scope wiki-mutation-drain")
    expect(instruction).toContain("--mutation-key wiki:project:panel:default")
    expect(instruction).not.toContain("--task-id")
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
      status: "waiting",
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
