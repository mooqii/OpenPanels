import { describe, expect, it, vi } from "vitest"
import {
  myDocumentOriginalUrl,
  originalPreviewKind,
  tryOpenBrowserWindow,
} from "./api"

describe("myDocumentOriginalUrl", () => {
  it("targets the immutable imported source", () => {
    expect(
      myDocumentOriginalUrl("http://localhost:43217", {
        id: "my-document:document/1",
      })
    ).toBe(
      "http://localhost:43217/api/my-documents/my-document%3Adocument%2F1/original"
    )
  })
})

describe("originalPreviewKind", () => {
  it("previews plain-text documents in the current window", () => {
    expect(
      originalPreviewKind({
        mimeType: "application/octet-stream",
        originalFileName: "component.mdx",
      })
    ).toBe("text")
    expect(
      originalPreviewKind({
        mimeType: "text/plain; charset=utf-8",
        originalFileName: "notes.unknown",
      })
    ).toBe("text")
  })

  it("recognizes image extensions even when the uploaded MIME type is missing", () => {
    expect(
      originalPreviewKind({
        mimeType: "application/octet-stream",
        originalFileName: "scan.tiff",
      })
    ).toBe("image")
  })

  it("leaves unsupported documents for browser or folder fallback", () => {
    expect(
      originalPreviewKind({
        mimeType: "application/octet-stream",
        originalFileName: "archive.zip",
      })
    ).toBeNull()
  })
})

describe("tryOpenBrowserWindow", () => {
  it("isolates a successfully opened browser window", () => {
    const openedWindow = { opener: {} } as Window
    const openWindow = vi.fn(() => openedWindow)

    expect(tryOpenBrowserWindow("http://localhost/document", openWindow)).toBe(
      true
    )
    expect(openWindow).toHaveBeenCalledWith(
      "http://localhost/document",
      "_blank"
    )
    expect(openedWindow.opener).toBeNull()
  })

  it("reports a blocked or failed browser window so callers can reveal the file", () => {
    expect(tryOpenBrowserWindow("http://localhost/document", () => null)).toBe(
      false
    )
    expect(
      tryOpenBrowserWindow("http://localhost/document", () => {
        throw new Error("blocked")
      })
    ).toBe(false)
  })

  it("does not reveal a file when the browser opened it but restricts opener access", () => {
    const openedWindow = Object.defineProperty({}, "opener", {
      set() {
        throw new Error("restricted")
      },
    }) as Window

    expect(
      tryOpenBrowserWindow("http://localhost/document", () => openedWindow)
    ).toBe(true)
  })
})
