import { describe, expect, it } from "vitest"
import { nextCollapsedModules, type WikiModule } from "./module-collapse"

const collapsed = (...modules: WikiModule[]) => new Set(modules)

describe("nextCollapsedModules", () => {
  it("collapses either Writing library module when both are open", () => {
    expect([...nextCollapsedModules(collapsed(), "generated")]).toEqual([
      "generated",
    ])
    expect([...nextCollapsedModules(collapsed(), "structured")]).toEqual([
      "structured",
    ])
  })

  it("reopens a collapsed module", () => {
    expect([
      ...nextCollapsedModules(collapsed("generated"), "generated"),
    ]).toEqual([])
  })

  it("swaps modules instead of leaving both collapsed", () => {
    expect([
      ...nextCollapsedModules(
        collapsed("generated", "structured"),
        "generated"
      ),
    ]).toEqual(["structured"])
  })
})
