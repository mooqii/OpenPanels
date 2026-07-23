import { describe, expect, it } from "vitest"
import { nextCollapsedModules, type WikiModule } from "./module-collapse"

const collapsed = (...modules: WikiModule[]) => new Set(modules)

describe("nextCollapsedModules", () => {
  it("collapses either Writing library module when both are open", () => {
    expect([...nextCollapsedModules(collapsed(), "myDocuments")]).toEqual([
      "myDocuments",
    ])
    expect([...nextCollapsedModules(collapsed(), "structured")]).toEqual([
      "structured",
    ])
  })

  it("reopens a collapsed module", () => {
    expect([
      ...nextCollapsedModules(collapsed("myDocuments"), "myDocuments"),
    ]).toEqual([])
  })

  it("swaps modules instead of leaving both collapsed", () => {
    expect([
      ...nextCollapsedModules(
        collapsed("myDocuments", "structured"),
        "myDocuments"
      ),
    ]).toEqual(["structured"])
  })
})
