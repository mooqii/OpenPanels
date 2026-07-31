import { readFileSync } from "node:fs"
import { describe, expect, it } from "vitest"

const studioHtml = readFileSync(
  new URL("../../index.html", import.meta.url),
  "utf8"
)

describe("Studio translation safety", () => {
  it("prevents browser translators from mutating the React-owned DOM", () => {
    expect(studioHtml).toContain('<html lang="en" translate="no">')
    expect(studioHtml).toContain('<meta name="google" content="notranslate">')
    expect(studioHtml).toContain('<body class="notranslate">')
  })
})
