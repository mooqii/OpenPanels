import { Button, cn, Label, Popover, Separator, Switch } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { useCallback, useMemo } from "react"
import type { Editor } from "../../editor"
import type { GeoShape } from "../../types/shapes"
import { ColorPicker, TRANSPARENT_BG } from "../ColorPicker"
import { NumberInput } from "../number-input"
import { CanvasToolbarTooltip as Tooltip } from "./CanvasToolbarTooltip"

interface ShadowItemProps {
  editor: Editor
  shape: GeoShape
}

const DEFAULT_SHADOW_COLOR = "rgba(0, 0, 0, 0.5)"

export function ShadowItem({ editor, shape }: ShadowItemProps) {
  const { t } = useLingui()
  const shadowBlur = (shape.props.shadowBlur as number) ?? 0
  const shadowOffsetX = (shape.props.shadowOffsetX as number) ?? 0
  const shadowOffsetY = (shape.props.shadowOffsetY as number) ?? 0
  const shadowOpacity = (shape.props.shadowOpacity as number) ?? 1
  const shadowColor =
    (shape.props.shadowColor as string) ?? DEFAULT_SHADOW_COLOR
  const shadowEnabled =
    shape.props.shadowEnabled ?? (shape.props.shadowBlur ?? 0) > 0

  const updateShadowProps = useCallback(
    (updates: Partial<GeoShape["props"]>) => {
      editor.updateShape(shape.id, {
        props: updates,
      })
    },
    [editor, shape.id]
  )

  const handleBlurChange = useCallback(
    (value: number) => {
      updateShadowProps({
        shadowBlur: value,
        shadowEnabled:
          shape.props.shadowEnabled === undefined ? value > 0 : shadowEnabled,
      })
    },
    [shape.props.shadowEnabled, shadowEnabled, updateShadowProps]
  )

  const handleOffsetXChange = useCallback(
    (value: number) => {
      updateShadowProps({ shadowOffsetX: value })
    },
    [updateShadowProps]
  )

  const handleOffsetYChange = useCallback(
    (value: number) => {
      updateShadowProps({ shadowOffsetY: value })
    },
    [updateShadowProps]
  )

  const handleOpacityChange = useCallback(
    (value: number) => {
      updateShadowProps({ shadowOpacity: value })
    },
    [updateShadowProps]
  )

  const handleColorChange = useCallback(
    (value: string) => {
      updateShadowProps({ shadowColor: value })
    },
    [updateShadowProps]
  )

  const handleToggle = useCallback(
    (enabled: boolean) => {
      updateShadowProps({ shadowEnabled: enabled })
    },
    [updateShadowProps]
  )

  const previewShadow = useMemo(() => {
    if (!shadowEnabled || shadowBlur <= 0) {
      return "none"
    }
    return `${shadowOffsetX}px ${shadowOffsetY}px ${shadowBlur}px ${shadowColor}`
  }, [shadowBlur, shadowColor, shadowEnabled, shadowOffsetX, shadowOffsetY])

  return (
    <Popover>
      <Tooltip>
        <Button
          aria-label={t`Shadow settings`}
          className={cn(
            "flex h-7 w-7 cursor-pointer items-center justify-center overflow-hidden rounded-full border-2 border-foreground p-0 outline-none",
            "ring-1 ring-border focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2"
          )}
          style={{
            background: TRANSPARENT_BG,
          }}
        >
          <div
            className="h-full w-full rounded-full border border-border bg-white"
            style={{
              boxShadow: previewShadow,
            }}
          />
        </Button>
        <Tooltip.Content>{t`Shadow`}</Tooltip.Content>
      </Tooltip>
      <Popover.Content offset={12}>
        <Popover.Dialog className="w-68 p-0">
          <div className="flex items-center justify-between px-4 py-3">
            <Popover.Heading>
              <span className="font-medium">{t`Shadow`}</span>
            </Popover.Heading>
            <Switch isSelected={shadowEnabled} onChange={handleToggle}>
              <Switch.Control>
                <Switch.Thumb />
              </Switch.Control>
              <Label className="text-sm">{t`Enabled`}</Label>
            </Switch>
          </div>
          <Separator />
          <div className="flex flex-col gap-3 px-4 py-3">
            <div className="flex items-center gap-2">
              <span className="w-16 text-muted text-xs">{t`Blur`}</span>
              <NumberInput
                className="rounded-lg px-2 py-1"
                dragVelocity={1}
                min={0}
                onChange={handleBlurChange}
                precision={1}
                step={1}
                value={shadowBlur}
              >
                <NumberInput.DragHandle />
                <NumberInput.Input className="max-w-18 text-right" />
                <NumberInput.Unit>px</NumberInput.Unit>
              </NumberInput>
            </div>
            <div className="flex items-center gap-2">
              <span className="w-16 text-muted text-xs">{t`Offset X`}</span>
              <NumberInput
                className="rounded-lg px-2 py-1"
                dragVelocity={1}
                min={-100}
                onChange={handleOffsetXChange}
                precision={1}
                step={1}
                value={shadowOffsetX}
              >
                <NumberInput.DragHandle />
                <NumberInput.Input className="max-w-18 text-right" />
                <NumberInput.Unit>px</NumberInput.Unit>
              </NumberInput>
            </div>
            <div className="flex items-center gap-2">
              <span className="w-16 text-muted text-xs">{t`Offset Y`}</span>
              <NumberInput
                className="rounded-lg px-2 py-1"
                dragVelocity={1}
                min={-100}
                onChange={handleOffsetYChange}
                precision={1}
                step={1}
                value={shadowOffsetY}
              >
                <NumberInput.DragHandle />
                <NumberInput.Input className="max-w-18 text-right" />
                <NumberInput.Unit>px</NumberInput.Unit>
              </NumberInput>
            </div>
            <div className="flex items-center gap-2">
              <span className="w-16 text-muted text-xs">{t`Opacity`}</span>
              <NumberInput
                className="rounded-lg px-2 py-1"
                dragVelocity={0.1}
                max={1}
                min={0}
                onChange={handleOpacityChange}
                precision={2}
                step={0.05}
                value={shadowOpacity}
              >
                <NumberInput.DragHandle />
                <NumberInput.Input className="max-w-18 text-right" />
              </NumberInput>
            </div>
          </div>
          <Separator />
          <div className="flex flex-col gap-5 p-4">
            <ColorPicker onChange={handleColorChange} value={shadowColor} />
          </div>
        </Popover.Dialog>
      </Popover.Content>
    </Popover>
  )
}
