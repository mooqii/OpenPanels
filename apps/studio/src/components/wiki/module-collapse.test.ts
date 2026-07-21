import { describe, expect, it } from "vitest"
import {
  nextCollapsedModules,
  serializeWikiCollapsedModules,
  type WikiModule,
  wikiCollapsedModulesFromStorage,
} from "./module-collapse"

const collapsed = (...modules: WikiModule[]) => new Set(modules)

describe("nextCollapsedModules", () => {
  it("collapses either Wiki document module when both are open", () => {
    expect([...nextCollapsedModules(collapsed(), "raw", false)]).toEqual([
      "raw",
    ])
    expect([...nextCollapsedModules(collapsed(), "generated", false)]).toEqual([
      "generated",
    ])
  })

  it("reopens a collapsed Wiki document module", () => {
    expect([...nextCollapsedModules(collapsed("raw"), "raw", false)]).toEqual(
      []
    )
  })

  it("swaps the open Wiki document module instead of collapsing both", () => {
    expect([
      ...nextCollapsedModules(collapsed("raw"), "generated", false),
    ]).toEqual(["generated"])
  })

  it("lets all three Writing library modules collapse", () => {
    expect([
      ...nextCollapsedModules(collapsed("structured"), "raw", true),
    ]).toEqual(["structured", "raw"])
    expect([
      ...nextCollapsedModules(
        collapsed("generated", "structured"),
        "raw",
        true
      ),
    ]).toEqual(["structured", "raw"])
    expect([
      ...nextCollapsedModules(collapsed("generated"), "generated", true),
    ]).toEqual([])
  })
})

describe("Wiki collapsed module persistence", () => {
  it("collapses generated documents by default", () => {
    expect([...wikiCollapsedModulesFromStorage(null)]).toEqual(["generated"])
  })

  it("restores a valid previous accordion state", () => {
    expect([...wikiCollapsedModulesFromStorage('["raw"]')]).toEqual(["raw"])
    expect([...wikiCollapsedModulesFromStorage("[]")]).toEqual([])
  })

  it("falls back safely and serializes in a stable order", () => {
    expect([...wikiCollapsedModulesFromStorage('["raw","generated"]')]).toEqual(
      ["generated"]
    )
    expect(serializeWikiCollapsedModules(collapsed("generated"))).toBe(
      '["generated"]'
    )
  })
})
