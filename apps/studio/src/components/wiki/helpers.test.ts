import { createElement } from "react"
import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import {
  conversionStatusTaskFilter,
  documentIndexStatus,
  indexStatusTaskFilter,
  WikiTaskStatusIcon,
} from "./helpers"

describe("document task-list filters", () => {
  it("opens active tasks for work in progress", () => {
    expect(conversionStatusTaskFilter("converting")).toBe("active")
    expect(indexStatusTaskFilter({ kind: "running", label: "Indexing" })).toBe(
      "active"
    )
  })

  it("opens pending tasks for queued and failed work", () => {
    expect(conversionStatusTaskFilter("queued")).toBe("pending")
    expect(conversionStatusTaskFilter("failed")).toBe("pending")
    expect(indexStatusTaskFilter({ kind: "pending", label: "Pending" })).toBe(
      "pending"
    )
    expect(indexStatusTaskFilter({ kind: "failed", label: "Failed" })).toBe(
      "pending"
    )
  })

  it("distinguishes cancelled indexing from pending work", () => {
    const status = documentIndexStatus(
      {
        conversion: {
          error: null,
          status: "not_required",
          taskId: null,
          updatedAt: "2026-07-16T00:00:00.000Z",
        },
        createdAt: "2026-07-16T00:00:00.000Z",
        id: "raw:cancelled",
        ingestionByWikiSpace: {
          "wiki:default": {
            error: null,
            status: "cancelled",
            taskId: "task:cancelled",
          },
        },
        markdownRef: "source.md",
        markdownVersion: 1,
        mimeType: "text/markdown",
        originalFileName: "cancelled.md",
        originalRef: "original/cancelled.md",
        sha256: "fixture",
        sizeBytes: 1,
        source: "user",
        title: "Cancelled",
        updatedAt: "2026-07-16T00:00:00.000Z",
      },
      "wiki:default"
    )

    expect(status).toEqual({
      kind: "cancelled",
      label: "Cancelled",
      taskId: "task:cancelled",
    })
    expect(indexStatusTaskFilter(status)).toBe("done")
  })

  it("opens completed tasks for indexed documents", () => {
    expect(indexStatusTaskFilter({ kind: "done", label: "Indexed" })).toBe(
      "done"
    )
  })

  it("distinguishes filtered, unrecorded, and unscheduled documents", () => {
    const document = {
      conversion: {
        error: null,
        status: "not_required",
        taskId: null,
        updatedAt: "2026-07-23T00:00:00.000Z",
      },
      createdAt: "2026-07-23T00:00:00.000Z",
      id: "raw:status",
      ingestionByWikiSpace: {
        "wiki:test": {
          error: null,
          reasonCode: "not_relevant",
          status: "filtered",
          taskId: "task:filtered",
        },
      },
      markdownRef: "source.md",
      markdownVersion: 1,
      mimeType: "text/markdown",
      originalFileName: "status.md",
      originalRef: "original/status.md",
      sha256: "fixture",
      sizeBytes: 1,
      source: "user",
      title: "Status",
      updatedAt: "2026-07-23T00:00:00.000Z",
    } as Parameters<typeof documentIndexStatus>[0]

    expect(documentIndexStatus(document, "wiki:test")).toEqual({
      kind: "filtered",
      label: "Filtered",
      taskId: "task:filtered",
    })
    document.ingestionByWikiSpace["wiki:test"].status = "unrecorded"
    expect(documentIndexStatus(document, "wiki:test").label).toBe("Unrecorded")
    expect(documentIndexStatus(document, "wiki:missing").label).toBe(
      "Unscheduled"
    )
  })
})

describe("WikiTaskStatusIcon", () => {
  it("uses sparkles for an AI-generated completion", () => {
    const markup = renderToStaticMarkup(
      createElement(WikiTaskStatusIcon, {
        doneIcon: "sparkles",
        filter: "done",
        kind: "done",
        label: "Succeeded",
        onOpenTasks: () => undefined,
        taskId: "task:generated",
      })
    )

    expect(markup).toContain("lucide-sparkles")
    expect(markup).not.toContain("lucide-circle-check-big")
  })
})
