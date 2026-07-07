import { useEffect, useMemo, useState } from "react"
import { Image as KonvaImage } from "react-konva"
import type { Asset } from "../types/assets"
import type { ImageShape } from "../types/shapes"

const IMAGE_LOAD_RETRY_DELAYS_MS = [250, 500, 1000, 2000, 4000] as const

interface ImageShapeRendererProps {
  asset?: Asset
  /** Natural size of source image when cropping */
  cropNaturalSize?: { width: number; height: number } | null
  draggable?: boolean
  /** Whether this shape is currently being cropped */
  isCropping?: boolean
  listening?: boolean
  onClick?: (e: any) => void
  onDragEnd?: (e: any) => void
  onDragMove?: (e: any) => void
  onDragStart?: (e: any) => void
  onMouseEnter?: (e: any) => void
  onMouseLeave?: (e: any) => void
  onTap?: (e: any) => void
  onTransformEnd?: (e: any) => void
  ref?: (node: any) => void
  /** Function to resolve asset URL for image shapes */
  resolveAsset?: (assetId: string) => string | undefined
  shape: ImageShape
}

function resolveRetryImageSrc(src: string, attempt: number) {
  if (attempt === 0 || src.startsWith("data:") || src.startsWith("blob:")) {
    return src
  }

  const separator = src.includes("?") ? "&" : "?"
  return `${src}${separator}canvasImageRetry=${attempt}`
}

export function ImageShapeRenderer({
  shape,
  asset,
  resolveAsset,
  draggable = false,
  listening = true,
  isCropping = false,
  cropNaturalSize,
  onClick,
  onDragStart,
  onDragEnd,
  onDragMove,
  onMouseEnter,
  onMouseLeave,
  onTap,
  onTransformEnd,
  ref,
}: ImageShapeRendererProps) {
  const props = shape.props
  const width = props.width ?? 100
  const height = props.height ?? 100
  const assetId = props.assetId
  const resolvedSrc =
    (assetId ? resolveAsset?.(assetId as string) : undefined) ??
    asset?.props?.src
  const src = resolvedSrc ?? ""

  const [image, setImage] = useState<HTMLImageElement | null>(null)

  useEffect(() => {
    if (!src) {
      setImage(null)
      return
    }

    let isCancelled = false
    let timeoutId: ReturnType<typeof setTimeout> | null = null
    let activeImage: HTMLImageElement | null = null

    const loadImage = (attempt: number) => {
      const img = new Image()
      activeImage = img
      img.crossOrigin = "anonymous"
      img.onload = () => {
        if (!isCancelled) {
          setImage(img)
        }
      }
      img.onerror = () => {
        if (isCancelled) {
          return
        }

        const retryDelay = IMAGE_LOAD_RETRY_DELAYS_MS[attempt]
        if (retryDelay != null) {
          timeoutId = setTimeout(() => {
            loadImage(attempt + 1)
          }, retryDelay)
          return
        }

        console.error(`Failed to load image: ${src}`)
        setImage(null)
      }
      img.src = resolveRetryImageSrc(src, attempt)
    }

    loadImage(0)

    return () => {
      isCancelled = true
      if (timeoutId !== null) {
        clearTimeout(timeoutId)
      }
      if (activeImage) {
        activeImage.onload = null
        activeImage.onerror = null
      }
    }
  }, [src])

  // Crop prop from shape (in image source coordinates)
  const existingCrop = props.crop as
    | { x: number; y: number; width: number; height: number }
    | undefined

  // Calculate display parameters for crop editing mode
  // When cropping, we show the FULL image, expanding beyond current shape bounds
  const displayParams = useMemo(() => {
    // Normal display (not cropping or no existing crop)
    if (!(isCropping && existingCrop && cropNaturalSize)) {
      return {
        x: props.x ?? 0,
        y: props.y ?? 0,
        width,
        height,
        crop: existingCrop,
      }
    }

    // Crop editing mode: show full image with proper positioning
    // The current shape shows the cropped region at its position
    // We need to expand to show the full image, offsetting so the cropped region stays in place

    // Calculate the scale from source to current display
    const scaleX = width / existingCrop.width
    const scaleY = height / existingCrop.height

    // Full image dimensions at current display scale
    const fullWidth = cropNaturalSize.width * scaleX
    const fullHeight = cropNaturalSize.height * scaleY

    // Offset to position full image so cropped region aligns with current shape position
    const currentX = props.x ?? 0
    const currentY = props.y ?? 0
    const offsetX = existingCrop.x * scaleX
    const offsetY = existingCrop.y * scaleY

    return {
      x: currentX - offsetX,
      y: currentY - offsetY,
      width: fullWidth,
      height: fullHeight,
      crop: undefined, // Don't apply crop - show full image
    }
  }, [
    isCropping,
    existingCrop,
    cropNaturalSize,
    props.x,
    props.y,
    width,
    height,
  ])

  return (
    <KonvaImage
      crop={displayParams.crop}
      draggable={draggable}
      height={displayParams.height}
      id={shape.id}
      image={image || undefined}
      listening={listening}
      onClick={onClick}
      onDragEnd={onDragEnd}
      onDragMove={onDragMove}
      onDragStart={onDragStart}
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
      onTap={onTap}
      onTransformEnd={onTransformEnd}
      opacity={props.opacity}
      ref={ref}
      rotation={props.rotation}
      scaleX={props.scaleX ?? 1}
      scaleY={props.scaleY ?? 1}
      width={displayParams.width}
      x={displayParams.x}
      y={displayParams.y}
    />
  )
}
