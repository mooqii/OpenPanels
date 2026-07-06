import type { Editor } from "../editor"
import {
  type Asset,
  AssetRecordType,
  type AssetStore,
  type ImageAssetProps,
} from "../types/assets"
import type { AssetId, ShapeId } from "../types/ids"
import { getViewportCenter } from "./coordinates"

/** Custom MIME type for canvas shape data */
export const SHAPE_MIME_TYPE = "application/x-creart-shapes"

/**
 * Convert a File/Blob to a data URL
 */
export function fileToDataUrl(file: File | Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(reader.result as string)
    reader.onerror = reject
    reader.readAsDataURL(file)
  })
}

/**
 * Get image dimensions from a data URL or URL
 */
export function getImageDimensions(
  src: string
): Promise<{ width: number; height: number }> {
  return new Promise((resolve, reject) => {
    const img = new Image()
    img.onload = () => resolve({ width: img.width, height: img.height })
    img.onerror = reject
    img.crossOrigin = "anonymous"
    img.src = src
  })
}

export function uploadAsset(
  editor: Editor,
  assetStore: AssetStore,
  assetId: AssetId,
  asset: Partial<Asset>,
  file: File,
  optimisticLocalSrc?: string
) {
  assetStore
    .upload(asset, file)
    .then((uploadResult) => {
      const { mimeType, src, meta } = uploadResult
      const props = {
        src,
      } as ImageAssetProps
      if (mimeType) {
        props.mimeType = mimeType
      }

      editor.run(() => {
        editor.updateAsset(assetId, {
          meta: meta || {},
          props,
        })
      })

      if (
        optimisticLocalSrc?.startsWith("blob:") &&
        optimisticLocalSrc !== src
      ) {
        URL.revokeObjectURL(optimisticLocalSrc)
      }
    })
    .catch((uploadError) => {
      console.error("Failed to upload image from URL:", uploadError)
    })
}

/**
 * Create an image asset and shape on the canvas from a file.
 * Uses optimistic UI: creates immediately with local blob URL, uploads in background.
 *
 * @param editor - The canvas editor instance
 * @param file - The image file
 * @param position - Where to place the image on canvas (top-left corner, or center if center=true)
 * @param assetStore - Optional asset store for uploading
 * @param center - If true, position is treated as the center point and adjusted accordingly
 * @returns The ID of the created shape
 */
export async function createImageFromFile(
  editor: Editor,
  file: File,
  position: { x: number; y: number },
  assetStore?: AssetStore,
  center?: boolean
): Promise<ShapeId> {
  const localBlobUrl = URL.createObjectURL(file)
  const { width, height } = await getImageDimensions(localBlobUrl)

  const assetId = AssetRecordType.createId()

  const assetProps = {
    name: file.name || "dropped-image",
    src: localBlobUrl,
    w: width,
    h: height,
    mimeType: file.type,
    isAnimated: false,
  }

  // Adjust position if centering is requested
  let finalPosition = position
  if (center) {
    finalPosition = {
      x: position.x - width / 2,
      y: position.y - height / 2,
    }
  }

  const shape = editor.run(() => {
    // Create asset immediately with local blob URL (optimistic)
    editor.createAssets([
      {
        id: assetId,
        typeName: "asset",
        type: "image",
        props: assetProps,
        meta: {},
      } as Asset,
    ])

    // Create the image shape immediately
    return editor.createShape({
      type: "image",
      props: {
        x: finalPosition.x,
        y: finalPosition.y,
        width,
        height,
        assetId,
      },
    })
  })

  editor.setSelectedShapes([shape.id as ShapeId])

  // Upload in background if asset store is provided
  if (assetStore) {
    const partialAsset: Partial<Asset> = {
      id: assetId,
      typeName: "asset",
      type: "image",
      props: assetProps,
      meta: {},
    }

    uploadAsset(editor, assetStore, assetId, partialAsset, file, localBlobUrl)
  }

  return shape.id as ShapeId
}

/**
 * Create an image asset and shape from a URL.
 * For remote URLs, we fetch and convert to data URL for the asset store.
 *
 * @param editor - The canvas editor instance
 * @param url - The image URL
 * @param position - Where to place the image on canvas
 * @param assetStore - Optional asset store for uploading
 */
export async function createImageFromUrl(
  editor: Editor,
  url: string,
  position: { x: number; y: number },
  assetStore?: AssetStore,
  center?: boolean
): Promise<ShapeId> {
  // Get dimensions from URL
  const { width, height } = await getImageDimensions(url)

  const assetId = AssetRecordType.createId()

  // Extract filename from URL
  const urlPath = new URL(url).pathname
  const name = urlPath.split("/").pop() || "image-from-url"

  const assetProps = {
    name,
    src: url,
    w: width,
    h: height,
    mimeType: "image/unknown",
    isAnimated: false,
  }

  // Create asset immediately with the URL
  editor.createAssets([
    {
      id: assetId,
      typeName: "asset",
      type: "image",
      props: assetProps,
      meta: {},
    } as Asset,
  ])

  // Adjust position if centering is requested
  let finalPosition = position
  if (center) {
    finalPosition = {
      x: position.x - width / 2,
      y: position.y - height / 2,
    }
  }

  // Create the image shape
  const shape = editor.createShape({
    type: "image",
    props: {
      x: finalPosition.x,
      y: finalPosition.y,
      width,
      height,
      assetId,
    },
  })

  editor.setSelectedShapes([shape.id as ShapeId])

  // If we have an asset store, fetch the image and upload it
  if (assetStore) {
    try {
      const response = await fetch(url)
      const blob = await response.blob()
      const file = new File([blob], name, { type: blob.type })

      const partialAsset: Partial<Asset> = {
        id: assetId,
        typeName: "asset",
        type: "image",
        props: { ...assetProps, mimeType: blob.type },
        meta: {},
      }

      uploadAsset(editor, assetStore, assetId, partialAsset, file)
    } catch (fetchError) {
      console.error("Failed to fetch image from URL:", fetchError)
      // Image still displays from original URL
    }
  }

  return shape.id as ShapeId
}

/**
 * Handle pasting images from clipboard
 * Uses optimistic UI: creates asset/shape immediately with local blob URL,
 * then uploads in background and updates the asset when complete.
 *
 * @param editor - The canvas editor instance
 * @param clipboardData - The clipboard data from paste event
 * @param assetStore - Optional asset store for uploading images
 * @returns true if image was pasted successfully, false otherwise
 */
export async function handleImagePaste(
  editor: Editor,
  clipboardData: DataTransfer,
  assetStore?: AssetStore
): Promise<boolean> {
  const items = Array.from(clipboardData.items)
  let imageFile: File | null = null

  // Look for image items in clipboard
  for (const item of items) {
    if (item.type.startsWith("image/")) {
      imageFile = item.getAsFile()
      break
    }
  }

  if (!imageFile) return false

  try {
    // Calculate viewport center in canvas coordinates
    const position = getViewportCenter(editor.stage) ?? { x: 400, y: 300 }

    await createImageFromFile(editor, imageFile, position, assetStore, true)
    return true
  } catch (error) {
    console.error("Failed to paste image:", error)
    return false
  }
}
