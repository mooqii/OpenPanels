import type { Key } from "@heroui/react"
import { ComboBox, Input, Label, ListBox } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { useCallback, useEffect, useMemo, useState } from "react"
import {
  TEXT_FONT_SIZE_MAX,
  TEXT_FONT_SIZE_MIN,
  TEXT_FONT_SIZE_OPTIONS,
} from "../../constants"
import type { Editor } from "../../editor"
import { getTextPropsWithUpdatedLayout } from "../../text-layout"
import {
  clampTextFontSize,
  getTextToolFromShape,
  toTextShapeFontSize,
} from "../../text-tool"
import { persistCanvasTool } from "../../tool-persistence"
import type { TextShape } from "../../types/shapes"

interface FontSizeSelectProps {
  editor: Editor
  shape: TextShape
}

const NUMERIC_INPUT_PATTERN = /^\d*$/

export function FontSizeSelect({ editor, shape }: FontSizeSelectProps) {
  const { t } = useLingui()
  const zoom = editor.getZoom()
  const currentTool = useMemo(
    () => getTextToolFromShape(shape, zoom),
    [shape, zoom]
  )
  const currentSize = currentTool.fontSize
  const [inputValue, setInputValue] = useState(() => currentSize.toString())
  const selectedKey = TEXT_FONT_SIZE_OPTIONS.some(
    (size) => size === currentSize
  )
    ? currentSize.toString()
    : null

  useEffect(() => {
    setInputValue(currentSize.toString())
  }, [currentSize])

  const commitValue = useCallback(
    (rawValue: string) => {
      const parsed = Number.parseInt(rawValue, 10)
      if (!Number.isFinite(parsed)) {
        setInputValue(currentSize.toString())
        return
      }

      const nextSize = clampTextFontSize(parsed)
      const nextProps = getTextPropsWithUpdatedLayout(
        shape.props,
        {
          fontSize: toTextShapeFontSize(nextSize, zoom),
        },
        {
          fallbackHeightMode: "auto",
          fallbackWidthMode: "manual",
        }
      )
      editor.updateShape(shape.id, {
        props: nextProps,
      })
      persistCanvasTool(
        getTextToolFromShape(shape, zoom, {
          fontSize: nextSize,
        })
      )
      setInputValue(nextSize.toString())
    },
    [currentSize, editor, shape, zoom]
  )

  const handleSelectionChange = useCallback(
    (key: Key | null) => {
      if (!key) return
      commitValue(String(key))
    },
    [commitValue]
  )

  return (
    <ComboBox
      allowsCustomValue
      aria-label={t`Font Size`}
      className="w-20"
      inputValue={inputValue}
      onInputChange={(value) => {
        if (NUMERIC_INPUT_PATTERN.test(value)) {
          setInputValue(value)
        }
      }}
      onSelectionChange={handleSelectionChange}
      selectedKey={selectedKey}
    >
      <Label className="sr-only">{t`Font Size`}</Label>
      <ComboBox.InputGroup>
        <Input
          aria-label={t`Font Size`}
          inputMode="numeric"
          max={TEXT_FONT_SIZE_MAX}
          min={TEXT_FONT_SIZE_MIN}
          onBlur={() => {
            commitValue(inputValue)
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              commitValue(inputValue)
            }
          }}
          placeholder={currentSize.toString()}
        />
        <ComboBox.Trigger />
      </ComboBox.InputGroup>
      <ComboBox.Popover>
        <ListBox aria-label={t`Font Size`}>
          {TEXT_FONT_SIZE_OPTIONS.map((size) => (
            <ListBox.Item
              id={size.toString()}
              key={size}
              textValue={size.toString()}
            >
              {size}
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </ListBox>
      </ComboBox.Popover>
    </ComboBox>
  )
}
