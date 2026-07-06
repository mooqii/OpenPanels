import { Button, cn } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { AlignCenter, AlignLeft, AlignRight } from "lucide-react"
import { useCallback } from "react"
import type { Editor } from "../../editor"
import { getTextToolFromShape, normalizeTextToolAlign } from "../../text-tool"
import { persistCanvasTool } from "../../tool-persistence"
import type { TextShape } from "../../types/shapes"
import { CanvasToolbarTooltip as Tooltip } from "./CanvasToolbarTooltip"

interface TextAlignSelectProps {
  editor: Editor
  shape: TextShape
}

export function TextAlignSelect({ editor, shape }: TextAlignSelectProps) {
  const { t } = useLingui()
  const currentAlign = normalizeTextToolAlign(shape.props.align)
  const alignments = [
    { value: "left", label: t`Left`, icon: AlignLeft },
    { value: "center", label: t`Center`, icon: AlignCenter },
    { value: "right", label: t`Right`, icon: AlignRight },
  ] as const

  const handleChange = useCallback(
    (key: "left" | "center" | "right") => {
      editor.updateShape(shape.id, {
        props: { align: key },
      })
      persistCanvasTool(
        getTextToolFromShape(shape, editor.getZoom(), {
          align: key,
        })
      )
    },
    [editor, shape]
  )

  return (
    <div
      aria-label={t`Text Alignment`}
      className="flex items-center gap-1"
      role="group"
    >
      {alignments.map((alignment) => {
        const AlignmentIcon = alignment.icon
        const isSelected = currentAlign === alignment.value

        return (
          <Tooltip key={alignment.value}>
            <Button
              aria-label={alignment.label}
              className={cn(
                "flex h-8 min-w-8 cursor-pointer items-center justify-center rounded-full px-0 text-text-tertiary transition-colors",
                isSelected
                  ? "text-foreground"
                  : "hover:bg-bg-muted hover:text-foreground"
              )}
              isIconOnly
              onPress={() => handleChange(alignment.value)}
              variant="ghost"
            >
              <AlignmentIcon size={16} />
            </Button>
            <Tooltip.Content>{alignment.label}</Tooltip.Content>
          </Tooltip>
        )
      })}
    </div>
  )
}
