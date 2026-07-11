import { describe, expect, it } from "vitest"
import { localeFromBrowserLanguages } from "./i18n"

describe("localeFromBrowserLanguages", () => {
  it("uses Chinese for Simplified Chinese and other Chinese environments", () => {
    expect(localeFromBrowserLanguages(["zh-CN"])).toBe("zh-CN")
    expect(localeFromBrowserLanguages(["zh-Hans", "en-US"])).toBe("zh-CN")
  })

  it("uses English for non-Chinese environments", () => {
    expect(localeFromBrowserLanguages(["en-US"])).toBe("en")
    expect(localeFromBrowserLanguages(["ja-JP", "en-US"])).toBe("en")
  })
})
