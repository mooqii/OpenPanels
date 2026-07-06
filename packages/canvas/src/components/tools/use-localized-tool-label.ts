import { useLingui } from "@lingui/react/macro"
import { useCallback } from "react"
import type { ToolConfigItem } from "./types"

export function useLocalizedToolLabel() {
  const { t } = useLingui()

  return useCallback(
    (toolId: ToolConfigItem["id"], fallbackLabel: string) => {
      switch (toolId) {
        case "select":
          return t`Select`
        case "hand":
          return t`Hand`
        case "rectangle":
          return t`Rectangle`
        case "ellipse":
          return t`Ellipse`
        case "line":
          return t`Line`
        case "pencil":
          return t`Pencil`
        case "brush":
          return t`Brush`
        case "marker":
          return t`Marker`
        case "pen":
          return t`Pen`
        case "text":
          return t`Text`
        case "connector":
          return t`Connector`
        case "image":
          return t`Add Image`
        default:
          return fallbackLabel
      }
    },
    [t]
  )
}
