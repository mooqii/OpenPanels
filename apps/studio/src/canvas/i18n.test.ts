import { describe, expect, it } from "vitest"
import {
  localeFromBrowserLanguages,
  translateMyOpenPanelsMessage,
} from "./i18n"

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

describe("task panel translations", () => {
  it("translates task names, statuses, and supporting labels", () => {
    expect(translateMyOpenPanelsMessage("zh-CN", "Tasks")).toBe("任务")
    expect(translateMyOpenPanelsMessage("zh-CN", "running")).toBe("进行中")
    expect(
      translateMyOpenPanelsMessage("zh-CN", "Generate Publication Cover")
    ).toBe("生成出版封面")
    expect(
      translateMyOpenPanelsMessage("zh-CN", "waiting for document conversion")
    ).toBe("等待文档转换")
    expect(translateMyOpenPanelsMessage("zh-CN", "Prerequisites")).toBe(
      "前置任务"
    )
  })

  it("keeps task panel messages unchanged in English", () => {
    expect(translateMyOpenPanelsMessage("en", "running")).toBe("running")
    expect(translateMyOpenPanelsMessage("en", "Write My Document")).toBe(
      "Write My Document"
    )
  })
})
