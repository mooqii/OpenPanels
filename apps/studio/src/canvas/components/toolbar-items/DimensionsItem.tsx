import { useCallback } from "react"
import type { Editor } from "../../editor"
import type { GeoShape } from "../../types/shapes"
import { DimensionInput } from "../DimensionInput"

interface DimensionsItemProps {
  editor: Editor
  shape: GeoShape
}

export function DimensionsItem({ editor, shape }: DimensionsItemProps) {
  const width = (shape.props.width as number) || 100
  const height = (shape.props.height as number) || 100

  const handleChange = useCallback(
    (newWidth: number, newHeight: number) => {
      editor.updateShape(shape.id, {
        props: { width: newWidth, height: newHeight },
      })
    },
    [editor, shape.id]
  )

  return (
    <DimensionInput height={height} onChange={handleChange} width={width} />
  )
}
