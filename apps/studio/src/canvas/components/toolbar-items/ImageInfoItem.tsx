import type { Editor } from "~/canvas/editor"
import type { Shape } from "../../types/shapes"
import { ImageMetadataItem } from "../ImageMetadataOverlay"

interface ImageInfoItemProps {
  editor: Editor
  shape: Shape | null
}

export function ImageInfoItem({ editor, shape }: ImageInfoItemProps) {
  if (!(shape?.type === "image" && shape.props.assetId)) {
    return null
  }

  const asset = editor.getAsset(shape.props.assetId)
  if (!(asset && asset.type === "image")) {
    return null
  }

  return <ImageMetadataItem asset={asset} />
}
