import type { JSONContent } from "@tiptap/core"
import type {
  TypesettingCanvasAsset,
  TypesettingPublication,
  TypesettingPublicationImage,
  TypesettingState,
} from "../types"

export const TYPESETTING_ASSET_DRAG_TYPE =
  "application/x-myopenpanels-canvas-asset"
export const TYPESETTING_AUTOSAVE_DELAY_MS = 500

export function emptyTypesettingDocument(): JSONContent {
  return {
    type: "doc",
    content: [{ type: "paragraph" }],
  }
}

export function createTypesettingPublication(
  id: string,
  timestamp: string
): TypesettingPublication {
  return {
    content: emptyTypesettingDocument(),
    covers: [],
    createdAt: timestamp,
    id,
    title: "",
    updatedAt: timestamp,
  }
}

export function isTypesettingDocumentEmpty(content: JSONContent): boolean {
  if (content.type === "image") return false
  if (typeof content.text === "string" && content.text.length > 0) return false
  return !(content.content ?? []).some(
    (child) => !isTypesettingDocumentEmpty(child)
  )
}

export function countTypesettingCharacters(content: JSONContent): number {
  const ownCharacters =
    typeof content.text === "string"
      ? Array.from(content.text).filter((character) => !/\s/u.test(character))
          .length
      : 0
  return (content.content ?? []).reduce(
    (total, child) => total + countTypesettingCharacters(child),
    ownCharacters
  )
}

export function plainTextToTypesettingContent(text: string): JSONContent[] {
  const paragraphs = text.replace(/\r\n?/g, "\n").split(/\n{2,}/)
  return paragraphs.map((paragraph) => {
    const lines = paragraph.split("\n")
    const content: JSONContent[] = []
    lines.forEach((line, index) => {
      if (index > 0) content.push({ type: "hardBreak" })
      if (line) content.push({ type: "text", text: line })
    })
    return content.length
      ? { type: "paragraph", content }
      : { type: "paragraph" }
  })
}

export function typesettingInsertPosition(
  documentSize: number,
  lastSelectionEnd: number | null
): number {
  return Math.max(0, Math.min(lastSelectionEnd ?? documentSize, documentSize))
}

export function typesettingTitleAfterDocumentInsert(
  publicationTitle: string,
  documentTitle: string
): string {
  return publicationTitle.trim() ? publicationTitle : documentTitle
}

export function moveTypesettingCover(
  covers: TypesettingPublicationImage[],
  from: number,
  to: number
): TypesettingPublicationImage[] {
  if (
    from === to ||
    from < 0 ||
    to < 0 ||
    from >= covers.length ||
    to >= covers.length
  ) {
    return covers
  }
  const next = [...covers]
  const [moved] = next.splice(from, 1)
  next.splice(to, 0, moved)
  return next
}

export function parseTypesettingAssetDrag(
  dataTransfer: Pick<DataTransfer, "getData">
): TypesettingCanvasAsset | null {
  const raw = dataTransfer.getData(TYPESETTING_ASSET_DRAG_TYPE)
  if (!raw) return null
  try {
    const asset = JSON.parse(raw) as Partial<TypesettingCanvasAsset>
    if (
      typeof asset.id !== "string" ||
      typeof asset.assetRef !== "string" ||
      typeof asset.projectId !== "string" ||
      typeof asset.canvasPanelId !== "string" ||
      typeof asset.src !== "string"
    ) {
      return null
    }
    return asset as TypesettingCanvasAsset
  } catch {
    return null
  }
}

export function groupTypesettingAssets(
  assets: TypesettingCanvasAsset[]
): Array<{
  projectId: string
  projectTitle: string
  assets: TypesettingCanvasAsset[]
}> {
  const groups = new Map<
    string,
    {
      projectId: string
      projectTitle: string
      assets: TypesettingCanvasAsset[]
    }
  >()
  for (const asset of assets) {
    const group = groups.get(asset.projectId) ?? {
      projectId: asset.projectId,
      projectTitle: asset.projectTitle,
      assets: [],
    }
    group.assets.push(asset)
    groups.set(asset.projectId, group)
  }
  return [...groups.values()]
}

export function mergeTypesettingConflict({
  deletedIds,
  dirtyIds,
  local,
  remote,
}: {
  deletedIds: ReadonlySet<string>
  dirtyIds: ReadonlySet<string>
  local: TypesettingState
  remote: TypesettingState
}): TypesettingState {
  const localById = new Map(
    local.publications.map((publication) => [publication.id, publication])
  )
  const publications = remote.publications
    .filter((publication) => !deletedIds.has(publication.id))
    .map((publication) =>
      dirtyIds.has(publication.id)
        ? (localById.get(publication.id) ?? publication)
        : publication
    )
  const remoteIds = new Set(publications.map((publication) => publication.id))
  for (const id of dirtyIds) {
    const publication = localById.get(id)
    if (publication && !remoteIds.has(id) && !deletedIds.has(id)) {
      publications.push(publication)
    }
  }
  return { schemaVersion: 1, publications }
}
