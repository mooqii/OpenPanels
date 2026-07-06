import { useCallback, useRef, useState } from "react"
import type { Editor } from "../editor"
import type { ShapeId } from "../types/ids"
import type { ImageShape } from "../types/shapes"

/**
 * Crop rectangle in image source coordinates
 */
export interface CropRect {
  height: number
  width: number
  x: number
  y: number
}

/**
 * Aspect ratio preset options
 */
export type AspectRatioPreset = "free" | "1:1" | "4:3" | "3:4" | "16:9" | "9:16"

/**
 * Cached natural dimensions for the image being cropped
 */
interface NaturalSize {
  height: number
  width: number
}

/**
 * Crop state managed by the hook
 */
interface CropState {
  /** Whether aspect ratio is locked */
  aspectRatioLock: boolean
  /** Current aspect ratio preset */
  aspectRatioPreset: AspectRatioPreset
  /** Current crop rectangle in image source coordinates */
  cropRect: CropRect | null
  /** The image shape being cropped */
  cropShapeId: ShapeId | null
  /** Cached natural size of the source image */
  naturalSize: NaturalSize | null
}

/**
 * Get aspect ratio value from preset
 */
function getAspectRatioValue(preset: AspectRatioPreset): number | null {
  switch (preset) {
    case "free":
      return null
    case "1:1":
      return 1
    case "4:3":
      return 4 / 3
    case "3:4":
      return 3 / 4
    case "16:9":
      return 16 / 9
    case "9:16":
      return 9 / 16
    default:
      return null
  }
}

/**
 * Hook to manage image crop state and operations.
 * Single source of truth for all crop-related state.
 */
export function useCrop(editor: Editor) {
  const [cropState, setCropState] = useState<CropState>({
    cropShapeId: null,
    cropRect: null,
    aspectRatioLock: false,
    aspectRatioPreset: "free",
    naturalSize: null,
  })

  // Cache for loaded image elements to get natural dimensions
  const imageCache = useRef<Map<string, HTMLImageElement>>(new Map())

  /**
   * Get the image shape being cropped
   */
  const getCroppingShape = useCallback((): ImageShape | null => {
    if (!cropState.cropShapeId) return null
    const shape = editor.getShape(cropState.cropShapeId)
    if (!shape || shape.type !== "image") return null
    return shape as ImageShape
  }, [editor, cropState.cropShapeId])

  /**
   * Get the natural dimensions of the image (source image size)
   * Uses cached naturalSize from crop state if available (most accurate)
   */
  const getImageNaturalSize = useCallback(
    (shape: ImageShape): { width: number; height: number } | null => {
      // Use cached natural size if we're cropping this shape
      if (cropState.naturalSize && cropState.cropShapeId === shape.id) {
        return cropState.naturalSize
      }

      const assetId = shape.props.assetId
      if (!assetId) return null

      const asset = editor.getAsset(assetId)
      if (!asset) return null

      // Try to get natural size from asset metadata
      const meta = asset.meta as { width?: number; height?: number } | undefined
      if (meta?.width && meta?.height) {
        return { width: meta.width, height: meta.height }
      }

      // Check if we have a cached image element with natural dimensions
      const cachedImg = imageCache.current.get(assetId as string)
      if (cachedImg?.naturalWidth && cachedImg?.naturalHeight) {
        return {
          width: cachedImg.naturalWidth,
          height: cachedImg.naturalHeight,
        }
      }

      // Fallback to shape dimensions (may be inaccurate if scaled)
      return {
        width: shape.props.width ?? 100,
        height: shape.props.height ?? 100,
      }
    },
    [editor, cropState.naturalSize, cropState.cropShapeId]
  )

  /**
   * Load image and get its natural dimensions
   */
  const loadImageNaturalSize = useCallback(
    (imageShape: ImageShape): Promise<NaturalSize> => {
      return new Promise((resolve) => {
        const assetId = imageShape.props.assetId
        if (!assetId) {
          // Fallback to shape dimensions
          resolve({
            width: imageShape.props.width ?? 100,
            height: imageShape.props.height ?? 100,
          })
          return
        }

        // Check cache first
        const cached = imageCache.current.get(assetId as string)
        if (cached?.naturalWidth && cached?.naturalHeight) {
          resolve({
            width: cached.naturalWidth,
            height: cached.naturalHeight,
          })
          return
        }

        // Get asset and resolve URL
        const asset = editor.getAsset(assetId)
        if (!asset) {
          resolve({
            width: imageShape.props.width ?? 100,
            height: imageShape.props.height ?? 100,
          })
          return
        }

        // Check asset metadata
        const meta = asset.meta as
          | { width?: number; height?: number }
          | undefined
        if (meta?.width && meta?.height) {
          resolve({ width: meta.width, height: meta.height })
          return
        }

        // Load image to get natural dimensions
        const assetStore = editor.getAssetStore()
        const src = assetStore?.resolve(asset) ?? (asset.props as any)?.src
        if (!src) {
          resolve({
            width: imageShape.props.width ?? 100,
            height: imageShape.props.height ?? 100,
          })
          return
        }

        const img = new Image()
        img.crossOrigin = "anonymous"
        img.onload = () => {
          imageCache.current.set(assetId as string, img)
          resolve({
            width: img.naturalWidth,
            height: img.naturalHeight,
          })
        }
        img.onerror = () => {
          resolve({
            width: imageShape.props.width ?? 100,
            height: imageShape.props.height ?? 100,
          })
        }
        img.src = src
      })
    },
    [editor]
  )

  /**
   * Enter crop mode for a specific image shape
   */
  const enterCropMode = useCallback(
    async (shapeId: ShapeId) => {
      const shape = editor.getShape(shapeId)
      if (!shape || shape.type !== "image") {
        console.warn("Cannot enter crop mode: shape is not an image")
        return
      }

      const imageShape = shape as ImageShape

      // Load actual image to get natural dimensions
      const naturalSize = await loadImageNaturalSize(imageShape)

      // Initialize crop rect from existing crop or full image bounds
      const existingCrop = imageShape.props.crop as CropRect | undefined
      const initialCropRect: CropRect = existingCrop ?? {
        x: 0,
        y: 0,
        width: naturalSize.width,
        height: naturalSize.height,
      }

      setCropState({
        cropShapeId: shapeId,
        cropRect: initialCropRect,
        aspectRatioLock: false,
        aspectRatioPreset: "free",
        naturalSize,
      })
    },
    [editor, loadImageNaturalSize]
  )

  /**
   * Exit crop mode without applying changes
   */
  const exitCropMode = useCallback(() => {
    setCropState({
      cropShapeId: null,
      cropRect: null,
      aspectRatioLock: false,
      aspectRatioPreset: "free",
      naturalSize: null,
    })
  }, [])

  /**
   * Update the crop rectangle
   */
  const setCropRect = useCallback(
    (rect: CropRect | ((prev: CropRect | null) => CropRect | null)) => {
      setCropState((prev) => {
        const newRect = typeof rect === "function" ? rect(prev.cropRect) : rect
        return {
          ...prev,
          cropRect: newRect,
        }
      })
    },
    []
  )

  /**
   * Set aspect ratio lock
   */
  const setAspectRatioLock = useCallback((locked: boolean) => {
    setCropState((prev) => ({
      ...prev,
      aspectRatioLock: locked,
    }))
  }, [])

  /**
   * Set aspect ratio preset and optionally adjust crop rect to match
   */
  const setAspectRatioPreset = useCallback((preset: AspectRatioPreset) => {
    setCropState((prev) => {
      const ratio = getAspectRatioValue(preset)

      // If no ratio constraint or no crop rect, just update preset
      if (!(ratio && prev.cropRect)) {
        return {
          ...prev,
          aspectRatioPreset: preset,
          aspectRatioLock: preset !== "free",
        }
      }

      // Adjust crop rect to match new aspect ratio
      // Keep the center point and adjust dimensions
      const currentRect = prev.cropRect
      const centerX = currentRect.x + currentRect.width / 2
      const centerY = currentRect.y + currentRect.height / 2
      const currentRatio = currentRect.width / currentRect.height

      let newWidth = currentRect.width
      let newHeight = currentRect.height

      if (currentRatio > ratio) {
        // Current is wider, reduce width
        newWidth = currentRect.height * ratio
      } else {
        // Current is taller, reduce height
        newHeight = currentRect.width / ratio
      }

      const newRect: CropRect = {
        x: centerX - newWidth / 2,
        y: centerY - newHeight / 2,
        width: newWidth,
        height: newHeight,
      }

      return {
        ...prev,
        aspectRatioPreset: preset,
        aspectRatioLock: preset !== "free",
        cropRect: newRect,
      }
    })
  }, [])

  /**
   * Apply the crop and exit crop mode
   */
  const applyCrop = useCallback(() => {
    const { cropShapeId, cropRect, naturalSize } = cropState
    if (!(cropShapeId && cropRect && naturalSize)) return

    const shape = editor.getShape(cropShapeId)
    if (!shape || shape.type !== "image") return

    const imageShape = shape as ImageShape

    // Current shape dimensions and position
    const currentWidth = imageShape.props.width ?? naturalSize.width
    const currentHeight = imageShape.props.height ?? naturalSize.height
    const currentX = imageShape.props.x ?? 0
    const currentY = imageShape.props.y ?? 0

    // Get existing crop (if re-cropping)
    const existingCrop = imageShape.props.crop as CropRect | undefined

    // Calculate the display scale factor
    // When there's an existing crop, scale is based on how that crop is displayed
    // When no existing crop, scale is based on how full image is displayed
    let displayScaleX: number
    let displayScaleY: number

    if (existingCrop) {
      // Re-cropping: scale = current display size / existing crop size in source
      displayScaleX = currentWidth / existingCrop.width
      displayScaleY = currentHeight / existingCrop.height
    } else {
      // First crop: scale = current display size / full image size
      displayScaleX = currentWidth / naturalSize.width
      displayScaleY = currentHeight / naturalSize.height
    }

    // Calculate new display dimensions based on new crop rect
    const newWidth = cropRect.width * displayScaleX
    const newHeight = cropRect.height * displayScaleY

    // Calculate the position of the full image's origin in canvas space
    // Then offset to where the new crop region should be
    let fullImageOriginX: number
    let fullImageOriginY: number

    if (existingCrop) {
      // The current shape position is where existingCrop.x/y appears
      // Full image origin = currentPos - (existingCrop offset in display coords)
      fullImageOriginX = currentX - existingCrop.x * displayScaleX
      fullImageOriginY = currentY - existingCrop.y * displayScaleY
    } else {
      // No existing crop - shape position IS the full image origin
      fullImageOriginX = currentX
      fullImageOriginY = currentY
    }

    // New shape position = full image origin + new crop offset
    const newX = fullImageOriginX + cropRect.x * displayScaleX
    const newY = fullImageOriginY + cropRect.y * displayScaleY

    // Update shape with crop and new dimensions/position
    editor.updateShape(cropShapeId, {
      props: {
        ...imageShape.props,
        crop: cropRect,
        width: newWidth,
        height: newHeight,
        x: newX,
        y: newY,
      },
    })

    // Exit crop mode
    exitCropMode()
  }, [cropState, editor, exitCropMode])

  /**
   * Reset crop to full image bounds
   */
  const resetCrop = useCallback(() => {
    const { naturalSize } = cropState
    const shape = getCroppingShape()
    if (!shape) return

    const fullRect: CropRect = {
      x: 0,
      y: 0,
      width: naturalSize?.width ?? shape.props.width ?? 100,
      height: naturalSize?.height ?? shape.props.height ?? 100,
    }

    setCropRect(fullRect)
  }, [cropState, getCroppingShape, setCropRect])

  /**
   * Get current aspect ratio value (for locked ratio operations)
   */
  const getCurrentAspectRatio = useCallback((): number | null => {
    if (!cropState.aspectRatioLock) return null
    return getAspectRatioValue(cropState.aspectRatioPreset)
  }, [cropState.aspectRatioLock, cropState.aspectRatioPreset])

  return {
    // State
    cropShapeId: cropState.cropShapeId,
    cropRect: cropState.cropRect,
    aspectRatioLock: cropState.aspectRatioLock,
    aspectRatioPreset: cropState.aspectRatioPreset,
    croppingShape: getCroppingShape(),
    naturalSize: cropState.naturalSize,

    // Actions
    enterCropMode,
    exitCropMode,
    setCropRect,
    setAspectRatioLock,
    setAspectRatioPreset,
    applyCrop,
    resetCrop,

    // Utilities
    getCurrentAspectRatio,
    getImageNaturalSize,
  }
}

export type UseCropReturn = ReturnType<typeof useCrop>
