import { describe, expect, it } from "vitest"
import {
  conversionStatusTaskFilter,
  documentIndexStatus,
  indexStatusTaskFilter,
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
    expect(
      indexStatusTaskFilter({ kind: "pending", label: "Pending index" })
    ).toBe("pending")
    expect(
      indexStatusTaskFilter({ kind: "failed", label: "Index failed" })
    ).toBe("pending")
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
        markdownRef: "raw/raw:cancelled/source.md",
        markdownVersion: 1,
        mimeType: "text/markdown",
        originalFileName: "cancelled.md",
        originalRef: "raw/raw:cancelled/original/cancelled.md",
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
      label: "Index cancelled",
      taskId: "task:cancelled",
    })
    expect(indexStatusTaskFilter(status)).toBe("done")
  })

  it("opens completed tasks for indexed documents", () => {
    expect(indexStatusTaskFilter({ kind: "done", label: "Indexed" })).toBe(
      "done"
    )
  })
})
