import { describe, expect, it } from "vitest"
import {
  externalBrowserPath,
  shouldShowOpenInBrowserPrompt,
} from "./browser-context"

describe("embedded panel browser context", () => {
  it("shows the browser prompt for known embedded browser hosts", () => {
    expect(
      shouldShowOpenInBrowserPrompt(
        "Mozilla/5.0 Chrome/126.0.0.0 Electron/31.0.0 Codex/1.0"
      )
    ).toBe(true)
  })

  it.each([
    "Mozilla/5.0 Chrome/126.0.0.0 Safari/537.36",
    "Mozilla/5.0 Edg/126.0.0.0 Chrome/126.0.0.0 Safari/537.36",
    "Mozilla/5.0 Firefox/128.0",
    "Mozilla/5.0 Version/17.5 Safari/605.1.15",
  ])("hides the prompt in a common standalone browser", (userAgent) => {
    expect(shouldShowOpenInBrowserPrompt(userAgent)).toBe(false)
  })

  it("keeps the current URL unchanged for the external browser", () => {
    expect(
      externalBrowserPath({
        hash: "#selection",
        pathname: "/wiki",
        search: "?tab=source",
      })
    ).toBe("/wiki?tab=source#selection")
  })
})
