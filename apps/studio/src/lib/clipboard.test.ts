import { afterEach, describe, expect, it, vi } from "vitest"
import { copyTextToClipboard } from "./clipboard"

describe("copyTextToClipboard", () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it("prefers the Clipboard API while the user activation is still available", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)
    const execCommand = vi.fn().mockReturnValue(true)
    vi.stubGlobal("navigator", { clipboard: { writeText } })
    vi.stubGlobal("document", createDocumentStub(execCommand))

    await expect(copyTextToClipboard("task instruction")).resolves.toBe(true)

    expect(writeText).toHaveBeenCalledWith("task instruction")
    expect(execCommand).not.toHaveBeenCalled()
  })

  it("falls back to the legacy copy command when Clipboard API access fails", async () => {
    const writeText = vi.fn().mockRejectedValue(new Error("NotAllowedError"))
    const execCommand = vi.fn().mockReturnValue(true)
    vi.stubGlobal("navigator", { clipboard: { writeText } })
    vi.stubGlobal("document", createDocumentStub(execCommand))

    await expect(copyTextToClipboard("task instruction")).resolves.toBe(true)

    expect(execCommand).toHaveBeenCalledWith("copy")
  })
})

function createDocumentStub(execCommand: ReturnType<typeof vi.fn>) {
  const textarea = {
    focus: vi.fn(),
    remove: vi.fn(),
    select: vi.fn(),
    setAttribute: vi.fn(),
    setSelectionRange: vi.fn(),
    style: {},
    value: "",
  }
  return {
    body: { append: vi.fn() },
    createElement: vi.fn().mockReturnValue(textarea),
    execCommand,
  }
}
