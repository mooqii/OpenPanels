import { describe, expect, it } from "vitest"
import { externalBrowserPath, isEmbeddedPanelView } from "./browser-context"

describe("embedded panel browser context", () => {
  it("only identifies explicitly marked embedded views", () => {
    expect(isEmbeddedPanelView("?myopenpanels-view=embedded")).toBe(true)
    expect(isEmbeddedPanelView("?myopenpanels-view=browser")).toBe(false)
    expect(isEmbeddedPanelView("")).toBe(false)
  })

  it("removes the embedded marker from the external browser URL", () => {
    expect(
      externalBrowserPath({
        hash: "#selection",
        pathname: "/wiki",
        search: "?myopenpanels-view=embedded&tab=source",
      })
    ).toBe("/wiki?tab=source#selection")
  })
})
