/**
 * Asset types for canvas media (images, videos, etc.)
 * Provides type-safe asset management with Konva-compatible properties.
 */

import type { ImageConfig } from "konva/lib/Node"
import { AssetId } from "./ids"

// =============================================================================
// Base Asset
// =============================================================================

/**
 * Base asset properties shared by all assets
 */
export interface BaseAssetProps {
  /** MIME type */
  mimeType: string
  /** Original file name */
  name: string
  /** Source URL (data URL, blob URL, or remote URL) */
  src: string
}

/**
 * Asset metadata
 */
export interface AssetMeta {
  [key: string]: unknown
}

// =============================================================================
// Image Asset
// =============================================================================

/**
 * Image asset props - extends Konva's ImageConfig concepts
 */
export interface ImageAssetProps
  extends BaseAssetProps,
    Omit<ImageConfig, "mimeType"> {
  /** Original image height */
  h: number
  /** Whether the image is animated (GIF, APNG, etc.) */
  isAnimated: boolean
  /** Original image width */
  w: number
}

/**
 * Image asset definition
 */
export interface ImageAsset {
  id: AssetId
  meta: AssetMeta
  props: ImageAssetProps
  type: "image"
  typeName: "asset"
}

// =============================================================================
// Video Asset
// =============================================================================

/**
 * Video asset props
 */
export interface VideoAssetProps extends BaseAssetProps {
  /** Duration in seconds */
  duration?: number
  /** Video height */
  h: number
  /** Thumbnail URL */
  thumbnail?: string
  /** Video width */
  w: number
}

/**
 * Video asset definition
 */
export interface VideoAsset {
  id: AssetId
  meta: AssetMeta
  props: VideoAssetProps
  type: "video"
  typeName: "asset"
}

// =============================================================================
// Bookmark Asset (for embedded content)
// =============================================================================

/**
 * Bookmark asset props
 */
export interface BookmarkAssetProps {
  /** Page description */
  description: string
  /** Favicon URL */
  favicon?: string
  /** Preview image URL */
  image?: string
  /** Page URL */
  src: string
  /** Page title */
  title: string
}

/**
 * Bookmark asset definition
 */
export interface BookmarkAsset {
  id: AssetId
  meta: AssetMeta
  props: BookmarkAssetProps
  type: "bookmark"
  typeName: "asset"
}

// =============================================================================
// Union Types
// =============================================================================

/**
 * All asset types union
 */
export type Asset = ImageAsset | VideoAsset | BookmarkAsset

/**
 * Asset type discriminator
 */
export type AssetType = Asset["type"]

/**
 * Extract asset by type
 */
export type AssetByType<T extends AssetType> = Extract<Asset, { type: T }>

/**
 * Asset props by type
 */
export type AssetProps = Asset["props"]

// =============================================================================
// AssetRecordType - Utility for creating assets (replaces tldraw's AssetRecordType)
// =============================================================================

/**
 * Utility object for creating asset IDs and records
 */
export const AssetRecordType = {
  /**
   * Create a new asset ID
   */
  createId: (): AssetId => AssetId.create(),

  /**
   * Create an image asset
   */
  createImageAsset: (
    props: Omit<ImageAssetProps, "mimeType"> & { mimeType?: string },
    meta: AssetMeta = {}
  ): ImageAsset => ({
    id: AssetId.create(),
    typeName: "asset",
    type: "image",
    props: {
      ...props,
      mimeType: props.mimeType || "image/png",
    },
    meta,
  }),

  /**
   * Create a video asset
   */
  createVideoAsset: (
    props: Omit<VideoAssetProps, "mimeType"> & { mimeType?: string },
    meta: AssetMeta = {}
  ): VideoAsset => ({
    id: AssetId.create(),
    typeName: "asset",
    type: "video",
    props: {
      ...props,
      mimeType: props.mimeType || "video/mp4",
    },
    meta,
  }),

  /**
   * Create a bookmark asset
   */
  createBookmarkAsset: (
    props: BookmarkAssetProps,
    meta: AssetMeta = {}
  ): BookmarkAsset => ({
    id: AssetId.create(),
    typeName: "asset",
    type: "bookmark",
    props,
    meta,
  }),
}

// =============================================================================
// Type Guards
// =============================================================================

export function isImageAsset(asset: Asset): asset is ImageAsset {
  return asset.type === "image"
}

export function isVideoAsset(asset: Asset): asset is VideoAsset {
  return asset.type === "video"
}

export function isBookmarkAsset(asset: Asset): asset is BookmarkAsset {
  return asset.type === "bookmark"
}

// =============================================================================
// AssetStore - Interface for managing asset uploads and resolution
// Patterned after tldraw's TLAssetStore
// =============================================================================

/**
 * Asset upload result returned from AssetStore.upload()
 */
export interface AssetUploadResult {
  /** Optional metadata about the asset */
  meta?: AssetMeta
  /** The MIME type of the asset */
  mimeType?: string
  /** The URL to access the uploaded asset */
  src: string
}

/**
 * AssetStore interface for handling asset uploads and URL resolution.
 * Similar to tldraw's TLAssetStore pattern.
 *
 * @example
 * ```ts
 * const assetStore: AssetStore = {
 *   async upload(asset, file) {
 *     const uploaded = await uploadToServer(file)
 *     return { src: uploaded.url, meta: uploaded.metadata }
 *   },
 *   resolve(asset) {
 *     retset.props.src
 *   }
 * }
 * ```
 */
export interface AssetStore {
  /**
   * Get the download URL for an asset.
   * @param asset - The asset record to download
   * @returns The URL string for downloading the asset
   */
  download(asset: Asset): Promise<string>

  /**
   * Resolve an asset to its URL.
   * @param asset - The asset record to resolve
   * @returns The URL string for accessing the asset
   */
  resolve(asset: Asset): string
  /**
   * Upload an asset to storage.
   * @param asset - The asset record being uploaded (partial, may not have id yet)
   * @param file - The file to upload
   * @returns The upload result with src URL and optional metadata
   */
  upload(asset: Partial<Asset>, file: File): Promise<AssetUploadResult>
}

/**
 * Default AssetStore that keeps images as data URLs (for testing/offline).
 * No actual upload performed, images are stored inline as base64.
 */
export class DataUrlAssetStore implements AssetStore {
  upload(_asset: Partial<Asset>, file: File): Promise<AssetUploadResult> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader()
      reader.onload = () => resolve({ src: reader.result as string })
      reader.onerror = reject
      reader.readAsDataURL(file)
    })
  }

  resolve(asset: Asset): string {
    // For data URLs, the src is stored directly in the asset props
    return (asset.props as any).src || ""
  }

  download(asset: Asset): Promise<string> {
    // For data URLs, return the src URL for download
    return Promise.resolve((asset.props as any).src || "")
  }
}
