import { describe, expect, it } from "vitest"
import {
  nextCollapsedLibraryModules,
  type TypesettingLibraryModule,
} from "./library-accordion"

const collapsed = (...modules: TypesettingLibraryModule[]) => new Set(modules)

describe("nextCollapsedLibraryModules", () => {
  it("collapses any module while another module remains open", () => {
    expect([
      ...nextCollapsedLibraryModules(collapsed(), "publications"),
    ]).toEqual(["publications"])
    expect([
      ...nextCollapsedLibraryModules(collapsed("publications"), "myDocuments"),
    ]).toEqual(["publications", "myDocuments"])
  })

  it("reopens a collapsed module", () => {
    expect([
      ...nextCollapsedLibraryModules(collapsed("myDocuments"), "myDocuments"),
    ]).toEqual([])
  })

  it("does not collapse the only expanded module", () => {
    expect([
      ...nextCollapsedLibraryModules(
        collapsed("publications", "myDocuments"),
        "assets"
      ),
    ]).toEqual(["publications", "myDocuments"])
  })
})
