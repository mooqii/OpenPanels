import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { createTypesettingPublication } from "../../lib/typesetting"
import type { TypesettingPublicationImage } from "../../types"
import { PublicationPreview } from "./PublicationPreview"

function cover(
  assetRef: string,
  mimeType: string
): TypesettingPublicationImage {
  return {
    assetRef,
    fileName: `${assetRef}.${mimeType.startsWith("video/") ? "mp4" : "png"}`,
    mimeType,
    source: { kind: "upload" },
    src: `/assets/${assetRef}`,
  }
}

describe("PublicationPreview covers", () => {
  it("makes images previewable while keeping video controls", () => {
    const publication = {
      ...createTypesettingPublication(
        "publication:preview",
        "2026-07-24T00:00:00Z"
      ),
      covers: [
        cover("cover:image", "image/png"),
        cover("cover:video", "video/mp4"),
      ],
      title: "Preview",
    }

    const markup = renderToStaticMarkup(
      <PublicationPreview
        onEdit={() => undefined}
        onOpenSources={() => undefined}
        publication={publication}
        transport={{ apiBase: "http://127.0.0.1:43217", kind: "http" }}
      />
    )

    expect(markup).toContain('aria-label="View cover"')
    expect(markup.match(/aria-label="View cover"/g)).toHaveLength(1)
    expect(markup).toContain("<video")
    expect(markup).toContain("controls")
    expect(markup).toContain('class="op-publishing-preview__scroll"')
  })
})
