import { extensionFromFileName } from "../../lib/api"
import type { WikiRawDocument } from "../../types"

export interface RawDocumentFormats {
  converted: string | null
  original: string
}

export function rawDocumentFormats(
  document: Pick<WikiRawDocument, "markdownRef" | "originalFileName"> & {
    conversion: Pick<WikiRawDocument["conversion"], "status">
  }
): RawDocumentFormats {
  const extension = extensionFromFileName(document.originalFileName)
  const original = [".markdown", ".md"].includes(extension)
    ? "MD"
    : extension.slice(1).toUpperCase()

  return {
    converted:
      document.markdownRef &&
      original !== "MD" &&
      document.conversion.status !== "not_required"
        ? "MD"
        : null,
    original,
  }
}
