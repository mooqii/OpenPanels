import { Dropdown, Label, Separator } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"

type MenuType = "shape" | "blank"

interface ContextMenuProps {
  canCopy: boolean
  canPaste: boolean
  isOpen: boolean
  menuPosition: { x: number; y: number }
  menuType: MenuType
  onAction: (key: string | number) => void
  onOpenChange: (isOpen: boolean) => void
}

export function ContextMenu({
  isOpen,
  menuPosition,
  menuType,
  canCopy,
  canPaste,
  onAction,
  onOpenChange,
}: ContextMenuProps) {
  const { t } = useLingui()

  const copy = t`Copy`
  const bringToFront = t`Bring to front`
  const sendToBack = t`Send to back`
  const del = t`Delete`
  const paste = t`Paste`
  const zoomIn = t`Zoom in`
  const zoomOut = t`Zoom out`

  return (
    <Dropdown isOpen={isOpen} onOpenChange={onOpenChange}>
      <Dropdown.Trigger
        data-testid="context-menu-trigger"
        style={{
          position: "absolute",
          left: menuPosition.x,
          top: menuPosition.y,
          width: 1,
          height: 1,
          opacity: 0,
        }}
      >
        <span />
      </Dropdown.Trigger>
      <Dropdown.Popover>
        {menuType === "shape" ? (
          <Dropdown.Menu onAction={onAction}>
            <Dropdown.Item id="copy" isDisabled={!canCopy} textValue={copy}>
              <Label>{copy}</Label>
            </Dropdown.Item>
            <Dropdown.Item
              id="bring-front"
              isDisabled={!canCopy}
              textValue={bringToFront}
            >
              <Label>{bringToFront}</Label>
            </Dropdown.Item>
            <Dropdown.Item
              id="send-back"
              isDisabled={!canCopy}
              textValue={sendToBack}
            >
              <Label>{sendToBack}</Label>
            </Dropdown.Item>
            <Separator />
            <Dropdown.Item
              id="delete"
              isDisabled={!canCopy}
              textValue={del}
              variant="danger"
            >
              <Label>{del}</Label>
            </Dropdown.Item>
          </Dropdown.Menu>
        ) : (
          <Dropdown.Menu onAction={onAction}>
            <Dropdown.Item id="paste" isDisabled={!canPaste} textValue={paste}>
              <Label>{paste}</Label>
            </Dropdown.Item>
            <Separator />
            <Dropdown.Item id="zoom-in" textValue={zoomIn}>
              <Label>{zoomIn}</Label>
            </Dropdown.Item>
            <Dropdown.Item id="zoom-out" textValue={zoomOut}>
              <Label>{zoomOut}</Label>
            </Dropdown.Item>
          </Dropdown.Menu>
        )}
      </Dropdown.Popover>
    </Dropdown>
  )
}
