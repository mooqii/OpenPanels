import { extensionFromFileName } from "../../lib/api"
import type { MyDocument } from "../../types"

function displayFormat(fileName: string): string {
  const extension = extensionFromFileName(fileName)
  return [".markdown", ".md"].includes(extension)
    ? "MD"
    : extension.slice(1).toUpperCase()
}

export function myDocumentFormats(
  document: Pick<MyDocument, "conversion" | "format" | "importSource">
): { converted: string | null; original: string | null } {
  if (!document.importSource) {
    return { converted: null, original: null }
  }
  const original = displayFormat(document.importSource.fileName)
  const current = document.format === "markdown" ? "MD" : "TXT"
  return {
    converted:
      document.conversion?.status === "ready" && original !== current
        ? current
        : null,
    original,
  }
}

export function countDocumentCharacters(content: string): number {
  return Array.from(content).filter((character) => !/\s/u.test(character))
    .length
}

export function myDocumentConversionDisplay(
  document: Pick<MyDocument, "conversion">
): {
  isFailed: boolean
  isLocked: boolean
  label: "converting" | "pending" | null
} {
  switch (document.conversion?.status) {
    case "queued":
      return { isFailed: false, isLocked: true, label: "pending" }
    case "converting":
      return { isFailed: false, isLocked: true, label: "converting" }
    case "failed":
      return { isFailed: true, isLocked: false, label: null }
    default:
      return { isFailed: false, isLocked: false, label: null }
  }
}
