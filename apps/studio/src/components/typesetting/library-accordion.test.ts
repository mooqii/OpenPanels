import { describe, expect, it } from "vitest"
import {
  nextCollapsedLibraryModules,
  type TypesettingLibraryModule,
} from "./library-accordion"

const collapsed = (...modules: TypesettingLibraryModule[]) => new Set(modules)

describe("nextCollapsedLibraryModules", () => {
  it("collapses any module while another module remains open", () => {
    expect([...nextCollapsedLibraryModules(collapsed(), "raw")]).toEqual([
      "raw",
    ])
    expect([
      ...nextCollapsedLibraryModules(collapsed("raw"), "generated"),
    ]).toEqual(["raw", "generated"])
  })

  it("reopens a collapsed module", () => {
    expect([...nextCollapsedLibraryModules(collapsed("raw"), "raw")]).toEqual(
      []
    )
  })

  it("does not collapse the only expanded module", () => {
    expect([
      ...nextCollapsedLibraryModules(
        collapsed("publications", "raw", "generated"),
        "assets"
      ),
    ]).toEqual(["publications", "raw", "generated"])
  })
})
