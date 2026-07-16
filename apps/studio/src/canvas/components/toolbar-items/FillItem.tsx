import { Button, cn, Popover, Separator } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { useCallback, useMemo } from "react"
import type { AssetId } from "~/canvas/types/ids"
import type { Editor } from "../../editor"
import type { GeoShape, ShapeFill } from "../../types/shapes"
import { DEFAULT_SOLID_FILL, toCssBackground } from "../../utils/fill"
import { TRANSPARENT_BG } from "../ColorPicker"
import { FillPanel } from "../fill/FillPanel"
import { CanvasToolbarTooltip as Tooltip } from "./CanvasToolbarTooltip"

interface FillColorItemProps {
  editor: Editor
  shape: GeoShape
}

export function FillItem({ editor, shape }: FillColorItemProps) {
  const { t } = useLingui()
  const fill = useMemo<ShapeFill>(() => {
    return shape.props.shapeFill ?? DEFAULT_SOLID_FILL
  }, [shape.props.shapeFill])

  const previewBackground = useMemo(() => {
    return toCssBackground(fill)
  }, [fill])

  const handleChange = useCallback(
    (newFill: ShapeFill) => {
      editor.updateShape(shape.id, {
        props: {
          shapeFill: newFill,
        } as Partial<GeoShape["props"]>,
      })
    },
    [editor, shape.id]
  )

  const getAsset = useCallback(
    (assetId: string) => {
      return editor.getAsset(assetId as AssetId)
    },
    [editor]
  )

  const handleUpload = useCallback(
    async (file: File): Promise<string> => {
      const assetStore = editor.getAssetStore()

      if (!assetStore) {
        throw new Error("No asset store configured")
      }

      const result = await assetStore.upload(
        {
          typeName: "asset",
          type: "image",
        },
        file
      )

      const [asset] = editor.createAssets([
        {
          typeName: "asset",
          type: "image",
          props: {
            name: file.name,
            src: result.src,
            mimeType: file.type,
            w: 0, // Will be updated when image loads
            h: 0,
            isAnimated: false,
          },
        },
      ])

      return asset.id
    },
    [editor]
  )

  return (
    <Popover>
      <Tooltip>
        <Button
          aria-label={t`Fill settings`}
          className={cn(
            "flex h-7 w-7 cursor-pointer items-center justify-center overflow-hidden rounded-full border-2 border-foreground p-0 outline-none",
            "ring-1 ring-border focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2"
          )}
          style={{
            background: TRANSPARENT_BG,
          }}
        >
          <div
            className="h-full w-full"
            style={{
              background: previewBackground || "transparent",
            }}
          />
        </Button>
        <Tooltip.Content>{t`Fill`}</Tooltip.Content>
      </Tooltip>
      <Popover.Content offset={12}>
        <Popover.Dialog className="w-72 p-0">
          <div className="flex items-center justify-between px-4 py-3">
            <Popover.Heading>
              <span className="font-medium">{t`Fill`}</span>
            </Popover.Heading>
          </div>
          <Separator />
          <div className="flex flex-col gap-5 p-4">
            <FillPanel
              getAsset={getAsset}
              onChange={handleChange}
              onUpload={handleUpload}
              value={fill}
            />
          </div>
        </Popover.Dialog>
      </Popover.Content>
    </Popover>
  )
}
