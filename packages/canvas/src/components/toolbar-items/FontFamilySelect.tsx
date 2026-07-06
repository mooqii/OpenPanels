import { ListBox, Select } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { useCallback, useEffect, useState } from "react"
import type { Editor } from "../../editor"
import { getTextPropsWithUpdatedLayout } from "../../text-layout"
import { getTextToolFromShape } from "../../text-tool"
import { persistCanvasTool } from "../../tool-persistence"
import type { TextShape } from "../../types/shapes"
import {
  GOOGLE_FONTS,
  isGoogleFont,
  loadGoogleFont,
  SYSTEM_FONTS,
} from "../../utils/google-fonts"

interface FontFamilySelectProps {
  editor: Editor
  shape: TextShape
}

export function FontFamilySelect({ editor, shape }: FontFamilySelectProps) {
  const { t } = useLingui()
  const currentFont = (shape.props.fontFamily as string) || "Arial"
  const [isLoading, setIsLoading] = useState(false)
  const currentFontWeight =
    getTextToolFromShape(shape, editor.getZoom()).fontWeight === "700"
      ? "700"
      : "400"

  // Load font if it's a Google Font
  useEffect(() => {
    if (isGoogleFont(currentFont)) {
      setIsLoading(true)
      loadGoogleFont(currentFont, currentFontWeight)
        .then(() => setIsLoading(false))
        .catch(() => setIsLoading(false))
    }
  }, [currentFont, currentFontWeight])

  const handleChange = useCallback(
    (key: string | null) => {
      if (!key) return

      const fontFamily = key as string
      const nextTool = getTextToolFromShape(shape, editor.getZoom(), {
        fontFamily,
      })
      const applyUpdate = () => {
        const nextProps = getTextPropsWithUpdatedLayout(
          shape.props,
          {
            fontFamily,
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

      // Load Google Font if needed
      if (isGoogleFont(fontFamily)) {
        setIsLoading(true)
        loadGoogleFont(
          fontFamily,
          nextTool.fontWeight === "700" ? "700" : "400"
        )
          .then(() => {
            setIsLoading(false)
            applyUpdate()
          })
          .catch(() => {
            setIsLoading(false)
            applyUpdate()
          })
      } else {
        applyUpdate()
      }
    },
    [editor, shape]
  )

  return (
    <Select
      aria-label={t`Font Family`}
      isDisabled={isLoading}
      onChange={handleChange as any}
      selectionMode="single"
      value={currentFont}
      variant="secondary"
    >
      <Select.Trigger>
        <Select.Value className="w-24 overflow-auto truncate">
          {isLoading ? t`Loading...` : currentFont}
        </Select.Value>
        <Select.Indicator />
      </Select.Trigger>
      <Select.Popover>
        <ListBox>
          {SYSTEM_FONTS.map((font) => (
            <ListBox.Item
              id={font}
              key={font}
              style={{ fontFamily: font }}
              textValue={font}
            >
              {font}
            </ListBox.Item>
          ))}
          {GOOGLE_FONTS.map((font) => (
            <ListBox.Item
              id={font.family}
              key={font.family}
              style={{ fontFamily: font.family }}
              textValue={font.family}
            >
              {font.family}
            </ListBox.Item>
          ))}
        </ListBox>
      </Select.Popover>
    </Select>
  )
}
