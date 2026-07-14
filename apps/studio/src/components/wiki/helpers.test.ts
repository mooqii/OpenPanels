import { describe, expect, it } from "vitest"
import { conversionStatusTaskFilter, indexStatusTaskFilter } from "./helpers"

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
})
