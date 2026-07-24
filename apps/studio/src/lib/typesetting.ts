import type { JSONContent } from "@tiptap/core"
import type {
  MyDocument,
  ProjectTask,
  TypesettingCanvasAsset,
  TypesettingPublication,
  TypesettingPublicationImage,
  TypesettingPublicationTitle,
  TypesettingState,
} from "../types"
import { taskCanCancel, taskDisplayPhase } from "./task-status"

export const TYPESETTING_ASSET_DRAG_TYPE =
  "application/x-myopenpanels-canvas-asset"
export const TYPESETTING_AUTOSAVE_DELAY_MS = 500
export const TYPESETTING_COVER_IMAGE_ACCEPT =
  ".png,.jpg,.jpeg,.webp,.gif,image/png,image/jpeg,image/webp,image/gif"
export const TYPESETTING_COVER_MEDIA_ACCEPT = [
  TYPESETTING_COVER_IMAGE_ACCEPT,
  ".mp4,.mov,.m4v,.webm,video/mp4,video/quicktime,video/webm",
].join(",")

const TYPESETTING_COVER_IMAGE_MIME_TYPES = new Set([
  "image/gif",
  "image/jpeg",
  "image/png",
  "image/webp",
])
const TYPESETTING_COVER_IMAGE_EXTENSIONS = new Set([
  "gif",
  "jpeg",
  "jpg",
  "png",
  "webp",
])
const TYPESETTING_COVER_VIDEO_MIME_TYPES = new Set([
  "video/mp4",
  "video/quicktime",
  "video/webm",
])
const TYPESETTING_COVER_VIDEO_EXTENSIONS = new Set([
  "m4v",
  "mov",
  "mp4",
  "webm",
])

export function isInsertableTypesettingDocument(document: {
  conversion?: Pick<NonNullable<MyDocument["conversion"]>, "status">
  mimeType: string
}): boolean {
  const conversionReady =
    !document.conversion ||
    document.conversion.status === "not_required" ||
    document.conversion.status === "ready"
  return document.mimeType.startsWith("text/") && conversionReady
}

export function isSupportedTypesettingCoverImage(file: {
  name: string
  type: string
}): boolean {
  if (TYPESETTING_COVER_IMAGE_MIME_TYPES.has(file.type.toLowerCase())) {
    return true
  }
  const extension = file.name.split(".").pop()?.toLowerCase()
  return Boolean(extension && TYPESETTING_COVER_IMAGE_EXTENSIONS.has(extension))
}

export function isSupportedTypesettingCoverMedia(file: {
  name: string
  type: string
}): boolean {
  if (isSupportedTypesettingCoverImage(file)) return true
  if (TYPESETTING_COVER_VIDEO_MIME_TYPES.has(file.type.toLowerCase())) {
    return true
  }
  const extension = file.name.split(".").pop()?.toLowerCase()
  return Boolean(extension && TYPESETTING_COVER_VIDEO_EXTENSIONS.has(extension))
}

export function isTypesettingCoverVideo(cover: { mimeType: string }): boolean {
  return cover.mimeType.toLowerCase().startsWith("video/")
}

export type TypesettingCoverTaskDisplayStatus =
  | "waiting"
  | "running"
  | "saving"
  | "failed"
  | "cancelled"

export type TypesettingLayoutTaskDisplayStatus =
  | "waiting"
  | "running"
  | "completed"
  | "failed"
  | "cancelled"

export type PublicationTitleTaskDisplayStatus =
  | "waiting"
  | "running"
  | "saving"
  | "failed"
  | "cancelled"

export function publicationTitleRequestPayload({
  instruction,
  publicationId,
  requestId,
  skillId,
}: {
  instruction: string
  publicationId: string
  requestId: string
  skillId: string
}): {
  instruction?: string
  publicationId: string
  requestId: string
  skillId: string
} {
  const trimmedInstruction = instruction.trim()
  return {
    ...(trimmedInstruction ? { instruction: trimmedInstruction } : {}),
    publicationId,
    requestId,
    skillId,
  }
}

export function publicationTitleTaskStatus(
  task: ProjectTask
): PublicationTitleTaskDisplayStatus {
  const phase = taskDisplayPhase(task)
  return phase === "succeeded" ? "saving" : phase
}

export function publicationLayoutRequestPayload({
  instruction,
  publicationId,
  requestId,
  skillId,
}: {
  instruction: string
  publicationId: string
  requestId: string
  skillId: string
}): {
  instruction?: string
  publicationId: string
  requestId: string
  skillId: string
} {
  const trimmedInstruction = instruction.trim()
  return {
    ...(trimmedInstruction ? { instruction: trimmedInstruction } : {}),
    publicationId,
    requestId,
    skillId,
  }
}

export function publicationLayoutTaskStatus(
  task: ProjectTask
): TypesettingLayoutTaskDisplayStatus {
  const phase = taskDisplayPhase(task)
  return phase === "succeeded" ? "completed" : phase
}

export function isTypesettingLayoutTaskActive(task: ProjectTask): boolean {
  return taskCanCancel(task)
}

export function latestTypesettingLayoutTask(
  tasks: ProjectTask[],
  publicationId: string
): ProjectTask | null {
  return (
    tasks
      .filter(
        (task) =>
          task.queue === "publication" &&
          task.type === "format_publication_content" &&
          task.targetId === publicationId
      )
      .sort((left, right) =>
        right.createdAt.localeCompare(left.createdAt)
      )[0] ?? null
  )
}

export function publicationCoverRequestPayload({
  instruction,
  publicationId,
  requestId,
  skillId,
}: {
  instruction: string
  publicationId: string
  requestId: string
  skillId: string
}): {
  instruction?: string
  publicationId: string
  requestId: string
  skillId: string
} {
  const trimmedInstruction = instruction.trim()
  return {
    ...(trimmedInstruction ? { instruction: trimmedInstruction } : {}),
    publicationId,
    requestId,
    skillId,
  }
}

export function publicationCoverTaskStatus(
  task: ProjectTask
): TypesettingCoverTaskDisplayStatus {
  const phase = taskDisplayPhase(task)
  return phase === "succeeded" ? "saving" : phase
}

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
  const selectedTitleId = `${id}:title:primary`
  return {
    content: emptyTypesettingDocument(),
    covers: [],
    createdAt: timestamp,
    id,
    selectedTitleId,
    tags: [],
    title: "",
    titles: [{ id: selectedTitleId, value: "" }],
    updatedAt: timestamp,
  }
}

export function typesettingPublicationTitles(
  publication: TypesettingPublication
): TypesettingPublicationTitle[] {
  if (publication.titles?.length) return publication.titles
  return [
    {
      id: `${publication.id}:title:primary`,
      value: publication.title,
    },
  ]
}

export function selectedPublicationTitleId(
  publication: TypesettingPublication
): string {
  const titles = typesettingPublicationTitles(publication)
  return titles.some(({ id }) => id === publication.selectedTitleId)
    ? (publication.selectedTitleId as string)
    : titles[0].id
}

export function selectPublicationTitle(
  publication: TypesettingPublication,
  titleId: string
): TypesettingPublication {
  const titles = typesettingPublicationTitles(publication)
  const selected = titles.find(({ id }) => id === titleId)
  if (!selected) return publication
  return {
    ...publication,
    selectedTitleId: selected.id,
    title: selected.value,
    titles,
  }
}

export function updatePublicationTitle(
  publication: TypesettingPublication,
  titleId: string,
  value: string
): TypesettingPublication {
  const selectedTitleId = selectedPublicationTitleId(publication)
  const titles = typesettingPublicationTitles(publication).map((title) =>
    title.id === titleId ? { ...title, value } : title
  )
  return {
    ...publication,
    selectedTitleId,
    title: titleId === selectedTitleId ? value : publication.title,
    titles,
  }
}

export function addPublicationTitle(
  publication: TypesettingPublication,
  title: TypesettingPublicationTitle
): TypesettingPublication {
  const titles = [...typesettingPublicationTitles(publication), title]
  return {
    ...publication,
    selectedTitleId: title.id,
    title: title.value,
    titles,
  }
}

export function removePublicationTitle(
  publication: TypesettingPublication,
  titleId: string,
  replacementTitleId?: string
): TypesettingPublication {
  const titles = typesettingPublicationTitles(publication)
  const removedIndex = titles.findIndex(({ id }) => id === titleId)
  if (removedIndex < 0) return publication
  if (titles.length === 1) {
    if (!replacementTitleId) return publication
    return {
      ...publication,
      selectedTitleId: replacementTitleId,
      title: "",
      titles: [{ id: replacementTitleId, value: "" }],
    }
  }
  const remaining = titles.filter(({ id }) => id !== titleId)
  const currentSelectedTitleId = selectedPublicationTitleId(publication)
  const selectedTitleId =
    currentSelectedTitleId === titleId
      ? remaining[Math.min(removedIndex, remaining.length - 1)].id
      : currentSelectedTitleId
  const selected = remaining.find(({ id }) => id === selectedTitleId)
  return {
    ...publication,
    selectedTitleId,
    title: selected?.value ?? remaining[0].value,
    titles: remaining,
  }
}

export function typesettingTagsFromInput(value: string): string[] {
  const tags: string[] = []
  const seen = new Set<string>()
  for (const part of value.split(/[,，\n]+/u)) {
    const tag = part.trim().replace(/^#+/u, "").trim()
    const key = tag.toLocaleLowerCase()
    if (!(tag && !seen.has(key))) continue
    seen.add(key)
    tags.push(tag)
  }
  return tags
}

export function appendTypesettingTags(
  current: string[],
  input: string
): string[] {
  const result = [...current]
  const seen = new Set(current.map((tag) => tag.toLocaleLowerCase()))
  for (const tag of typesettingTagsFromInput(input)) {
    const key = tag.toLocaleLowerCase()
    if (seen.has(key)) continue
    seen.add(key)
    result.push(tag)
  }
  return result
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

export function typesettingImagesToContent(
  images: TypesettingPublicationImage[]
): JSONContent[] {
  return images.map((image) => ({
    type: "image",
    attrs: {
      alt: image.fileName,
      assetRef: image.assetRef,
      height: image.height,
      src: image.src,
      title: image.fileName,
      width: image.width,
    },
  }))
}

export function typesettingImageClickSide(
  clientX: number,
  imageLeft: number,
  imageRight: number
): "after" | "before" | "inside" {
  if (clientX < imageLeft) return "before"
  if (clientX > imageRight) return "after"
  return "inside"
}

export function typesettingInsertPosition(
  documentSize: number,
  lastSelectionEnd: number | null
): number {
  return Math.max(0, Math.min(lastSelectionEnd ?? documentSize, documentSize))
}

export function publicationTitleAfterDocumentInsert(
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
  contentDirtyIds,
  deletedCoverAssetRefs,
  deletedIds,
  dirtyIds,
  local,
  remote,
}: {
  contentDirtyIds?: ReadonlySet<string>
  deletedCoverAssetRefs?: ReadonlyMap<string, ReadonlySet<string>>
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
    .map((publication) => {
      if (!dirtyIds.has(publication.id)) return publication
      const localPublication = localById.get(publication.id)
      if (!localPublication) return publication
      const deletedCovers = deletedCoverAssetRefs?.get(publication.id)
      const localAssetRefs = new Set(
        localPublication.covers.map((cover) => cover.assetRef)
      )
      const generatedCovers = publication.covers.filter(
        (cover) =>
          cover.source.kind === "generated" &&
          !localAssetRefs.has(cover.assetRef) &&
          !deletedCovers?.has(cover.assetRef)
      )
      return {
        ...localPublication,
        content:
          contentDirtyIds === undefined || contentDirtyIds.has(publication.id)
            ? localPublication.content
            : publication.content,
        covers: [...localPublication.covers, ...generatedCovers],
      }
    })
  const remoteIds = new Set(publications.map((publication) => publication.id))
  for (const id of dirtyIds) {
    const publication = localById.get(id)
    if (publication && !remoteIds.has(id) && !deletedIds.has(id)) {
      publications.push(publication)
    }
  }
  return { publications }
}
