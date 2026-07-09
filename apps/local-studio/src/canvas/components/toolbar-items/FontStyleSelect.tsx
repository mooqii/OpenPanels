import { Button, cn } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { Bold } from "lucide-react"
import { useCallback } from "react"
import type { Editor } from "../../editor"
import { getTextPropsWithUpdatedLayout } from "../../text-layout"
import { getTextToolFromShape, toTextShapeFontStyle } from "../../text-tool"
import { persistCanvasTool } from "../../tool-persistence"
import type { TextShape } from "../../types/shapes"
import { isGoogleFont, loadGoogleFont } from "../../utils/google-fonts"
import { CanvasToolbarTooltip as Tooltip } from "./CanvasToolbarTooltip"

interface FontStyleSelectProps {
  editor: Editor
  shape: TextShape
}

export function FontStyleSelect({ editor, shape }: FontStyleSelectProps) {
  const { t } = useLingui()
  const currentTool = getTextToolFromShape(shape, editor.getZoom())
  const isSelected = currentTool.fontWeight === "700"

  const handleChange = useCallback(() => {
    const nextTool = getTextToolFromShape(shape, editor.getZoom(), {
      fontWeight: isSelected ? "normal" : "700",
    })
    const applyUpdate = () => {
      const nextProps = getTextPropsWithUpdatedLayout(
        shape.props,
        {
          fontStyle: toTextShapeFontStyle(nextTool.fontWeight),
        },
        {
          fallbackHeightMode: "auto",
          fallbackWidthMode: "manual",
        }
      )
      editor.updateShape(shape.id, {
        props: nextProps,
      })
      persistCanvasTool(nextTool)
    }

    if (isGoogleFont(nextTool.fontFamily)) {
      loadGoogleFont(
        nextTool.fontFamily,
        nextTool.fontWeight === "700" ? "700" : "400"
      )
        .then(applyUpdate)
        .catch(applyUpdate)
      return
    }

    applyUpdate()
  }, [editor, isSelected, shape])

  return (
    <Tooltip>
      <Button
        aria-label={t`Font Style`}
        className={cn(
          "flex h-8 min-w-8 cursor-pointer items-center justify-center rounded-full px-0 text-text-tertiary transition-colors",
          isSelected
            ? "text-foreground"
            : "hover:bg-bg-muted hover:text-foreground"
        )}
        isIconOnly
        onPress={handleChange}
        variant="ghost"
      >
        <Bold size={16} />
      </Button>
      <Tooltip.Content>{t`Font Style`}</Tooltip.Content>
    </Tooltip>
  )
}
