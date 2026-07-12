import type { Editor } from "./editor"
import type { Shape as CanvasShape } from "./types/shapes"
import { getShapesBounds } from "./utils/coordinates"

export interface CanvasSelectionShape {
  asset?: {
    assetRef?: string | null
    h?: number | null
    id: string
    mimeType?: string | null
    name?: string | null
    src?: string | null
    w?: number | null
  } | null
  bounds: { height: number; width: number; x: number; y: number }
  id: string
  parentId: string
  props: unknown
  type: string
}

export interface CanvasSelectionSnapshot {
  assetRef?: string | null
  selectedShapeIds: string[]
  selectedShapes: CanvasSelectionShape[]
}

export function summarizeSelectionShape(
  editor: Editor,
  shape: CanvasShape
): CanvasSelectionShape {
  const asset =
    shape.type === "image" && shape.props.assetId
      ? editor.getAsset(shape.props.assetId)
      : null
  const bounds = getShapesBounds([shape])
  return {
    id: shape.id,
    type: shape.type,
    parentId: shape.parentId,
    props: shape.props,
    bounds,
    asset: asset
      ? {
          id: asset.id,
          assetRef: assetRefFromAsset(asset),
          name: "name" in asset.props ? asset.props.name : null,
          src: "src" in asset.props ? asset.props.src : null,
          w: "w" in asset.props ? asset.props.w : null,
          h: "h" in asset.props ? asset.props.h : null,
          mimeType: "mimeType" in asset.props ? asset.props.mimeType : null,
        }
      : null,
  }
}

function assetRefFromAsset(asset: { meta?: unknown; props?: unknown }) {
  if (
    asset.meta &&
    typeof asset.meta === "object" &&
    "assetRef" in asset.meta &&
    typeof asset.meta.assetRef === "string"
  ) {
    return asset.meta.assetRef
  }
  const props = asset.props
  if (!(props && typeof props === "object" && "src" in props)) return null
  const src = props.src
  if (typeof src !== "string") return null
  const match = src.match(/^\/api\/panels\/([^/]+)\/([^/]+)\/assets\/(.+)$/)
  if (!match) return null
  const projectId = decodeURIComponent(match[1] ?? "")
  const panelId = decodeURIComponent(match[2] ?? "")
  const assetPath = (match[3] ?? "").split("/").map(decodeURIComponent)
  return [
    "projects",
    projectId,
    "panels",
    panelId,
    "assets",
    ...assetPath,
  ].join("/")
}
