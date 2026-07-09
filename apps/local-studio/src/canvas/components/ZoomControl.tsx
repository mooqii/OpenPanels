import { Button, Dropdown, Kbd, Label, Separator } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { Minus, Plus } from "lucide-react"
import { useCallback, useEffect, useState } from "react"
import { useEditor } from "../EditorContext"

const zoomOptions = { animation: { duration: 200 } }

export function ZoomControl() {
  const { t } = useLingui()
  const editor = useEditor()
  const [zoom, setZoom] = useState(() => editor.getZoom())

  useEffect(() => {
    const stage = editor.stage
    if (!stage) return

    // Update zoom when stage scale changes
    const updateZoom = () => {
      setZoom(stage.scaleX())
    }

    // Listen to scale changes - Konva fires separate events
    stage.on("scaleXChange", updateZoom)
    stage.on("scaleYChange", updateZoom)

    // Initial update
    updateZoom()

    return () => {
      stage.off("scaleXChange", updateZoom)
      stage.off("scaleYChange", updateZoom)
    }
  }, [editor])

  const zoomPercentage = Math.round(zoom * 100)

  const handleZoomIn = useCallback(() => {
    editor.zoomIn(undefined, zoomOptions)
  }, [editor])

  const handleZoomOut = useCallback(() => {
    editor.zoomOut(undefined, zoomOptions)
  }, [editor])

  const handleSetZoom = useCallback(
    (scale: number) => {
      editor.zoom(scale, undefined, zoomOptions)
    },
    [editor]
  )

  const handleZoomToFit = useCallback(() => {
    editor.zoomToFit({ animation: { duration: 250 } })
  }, [editor])

  const formatKeyboardShortcut = (key: string) => {
    const isMac = navigator.platform.toUpperCase().indexOf("MAC") >= 0
    return isMac ? `⌘ ${key}` : `Ctrl+${key}`
  }

  const handleMenuAction = useCallback(
    (key: string | number) => {
      switch (key) {
        case "zoom-in":
          handleZoomIn()
          break
        case "zoom-out":
          handleZoomOut()
          break
        case "fit-to-screen":
          handleZoomToFit()
          break
        case "zoom-50":
          handleSetZoom(0.5)
          break
        case "zoom-100":
          handleSetZoom(1)
          break
        case "zoom-200":
          handleSetZoom(2)
          break
        default:
          break
      }
    },
    [handleZoomIn, handleZoomOut, handleZoomToFit, handleSetZoom]
  )

  return (
    <div
      className="fixed bottom-2 flex items-center gap-0.5 rounded-full bg-canvas-toolbar px-1 py-0.5 shadow backdrop-blur-lg"
      style={{
        left: "calc(var(--main-layout-sidebar-offset, var(--home-main-offset, 0px)) + 0.5rem)",
      }}
    >
      <Button
        aria-label={t`Zoom out`}
        isIconOnly
        onClick={handleZoomOut}
        size="sm"
        variant="ghost"
      >
        <Minus size={14} strokeWidth={2} />
      </Button>

      <Dropdown>
        <Button
          className="w-14 min-w-14 cursor-pointer px-2"
          size="sm"
          variant="ghost"
        >
          <span className="min-w-10">{zoomPercentage}%</span>
        </Button>
        <Dropdown.Popover>
          <Dropdown.Menu onAction={handleMenuAction}>
            <Dropdown.Item id="zoom-in" textValue={t`Zoom in`}>
              <Label className="flex-1">{t`Zoom in`}</Label>
              <Kbd slot="keyboard">{formatKeyboardShortcut("+")}</Kbd>
            </Dropdown.Item>
            <Dropdown.Item id="zoom-out" textValue={t`Zoom out`}>
              <Label className="flex-1">{t`Zoom out`}</Label>
              <Kbd slot="keyboard">{formatKeyboardShortcut("-")}</Kbd>
            </Dropdown.Item>
            <Dropdown.Item id="fit-to-screen" textValue={t`Fit to Screen`}>
              <Label className="flex-1">{t`Fit to Screen`}</Label>
            </Dropdown.Item>
            <Separator />
            <Dropdown.Item id="zoom-50" textValue={t`Zoom to 50%`}>
              <Label className="flex-1">{t`Zoom to 50%`}</Label>
            </Dropdown.Item>
            <Dropdown.Item id="zoom-100" textValue={t`Zoom to 100%`}>
              <Label className="flex-1">{t`Zoom to 100%`}</Label>
            </Dropdown.Item>
            <Dropdown.Item id="zoom-200" textValue={t`Zoom to 200%`}>
              <Label className="flex-1">{t`Zoom to 200%`}</Label>
            </Dropdown.Item>
          </Dropdown.Menu>
        </Dropdown.Popover>
      </Dropdown>

      <Button
        aria-label={t`Zoom in`}
        isIconOnly
        onClick={handleZoomIn}
        size="sm"
        variant="ghost"
      >
        <Plus size={14} strokeWidth={2} />
      </Button>
    </div>
  )
}
