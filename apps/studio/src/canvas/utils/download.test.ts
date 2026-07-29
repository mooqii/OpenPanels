import { afterEach, describe, expect, it, vi } from "vitest"
import { downloadUrlAsFile } from "./download"

describe("downloadUrlAsFile", () => {
  afterEach(() => {
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it("downloads in the current tab instead of opening a new one", () => {
    const click = vi.fn()
    const remove = vi.fn()
    const link = { click, download: "", href: "", remove, target: "" }
    const appendChild = vi.fn()

    vi.stubGlobal("document", {
      body: { appendChild },
      createElement: vi.fn(() => link),
    })

    downloadUrlAsFile("/api/assets/image/content", "selection.png")

    expect(link).toMatchObject({
      download: "selection.png",
      href: "/api/assets/image/content",
      target: "",
    })
    expect(appendChild).toHaveBeenCalledWith(link)
    expect(click).toHaveBeenCalledOnce()
    expect(remove).toHaveBeenCalledOnce()
  })
})
