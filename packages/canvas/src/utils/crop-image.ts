import type { CropRect } from "../hooks/use-crop"

/**
 * Options for exporting a cropped image
 */
export interface ExportCroppedImageOptions {
  /** Crop rectangle in source image coordinates */
  crop: CropRect
  /** Output mime type (default: "image/png") */
  mimeType?: "image/png" | "image/jpeg" | "image/webp"
  /** Quality for JPEG/WebP (0-1, default: 0.92) */
  quality?: number
  /** Source image URL (data URL, blob URL, or remote URL) */
  src: string
}

/**
 * Load an image from a URL with CORS handling.
 * For non-data URLs, sets crossOrigin to "anonymous" to allow canvas export.
 */
function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image()

    // Set crossOrigin for non-data URLs to enable canvas export
    if (!src.startsWith("data:")) {
      img.crossOrigin = "anonymous"
    }

    img.onload = () => resolve(img)
    img.onerror = () =>
      reject(new Error(`Failed to load image: ${src.slice(0, 100)}...`))
    img.src = src
  })
}

/**
 * Export a cropped region of an image as a data URL.
 *
 * Loads the source image, draws the specified crop region to an offscreen canvas,
 * and returns the result as a data URL.
 *
 * @param options - Export options including source URL, crop rect, and output format
 * @returns Data URL of the cropped image, or null if export fails
 *
 * @example
 * ```ts
 * const croppedDataUrl = await exportCroppedImageDataUrl({
 *   src: "https://example.com/image.jpg",
 *   crop: { x: 100, y: 50, width: 200, height: 150 },
 * })
 * ```
 */
export async function exportCroppedImageDataUrl(
  options: ExportCroppedImageOptions
): Promise<string | null> {
  const { src, crop, mimeType = "image/png", quality = 0.92 } = options

  // Validate crop dimensions
  if (crop.width <= 0 || crop.height <= 0) {
    console.warn("Invalid crop dimensions:", crop)
    return null
  }

  try {
    const img = await loadImage(src)

    // Create offscreen canvas with crop dimensions
    const canvas = document.createElement("canvas")
    canvas.width = crop.width
    canvas.height = crop.height

    const ctx = canvas.getContext("2d")
    if (!ctx) {
      console.error("Failed to get canvas 2D context")
      return null
    }

    // Draw the cropped region of the source image
    // drawImage(image, sx, sy, sWidth, sHeight, dx, dy, dWidth, dHeight)
    ctx.drawImage(
      img,
      crop.x, // Source X
      crop.y, // Source Y
      crop.width, // Source width
      crop.height, // Source height
      0, // Destination X
      0, // Destination Y
      crop.width, // Destination width
      crop.height // Destination height
    )

    // Export to data URL
    const dataUrl = canvas.toDataURL(mimeType, quality)

    // Verify the export succeeded (toDataURL returns "data:," on failure)
    if (dataUrl === "data:," || dataUrl.length < 100) {
      console.error("Canvas export failed - possibly tainted by CORS")
      return null
    }

    return dataUrl
  } catch (error) {
    console.error("Failed to export cropped image:", error)
    return null
  }
}

/**
 * Check if a shape has a valid crop applied.
 */
export function hasCrop(crop: unknown): crop is CropRect {
  if (!crop || typeof crop !== "object") return false
  const c = crop as Record<string, unknown>
  return (
    typeof c.x === "number" &&
    typeof c.y === "number" &&
    typeof c.width === "number" &&
    typeof c.height === "number" &&
    c.width > 0 &&
    c.height > 0
  )
}
