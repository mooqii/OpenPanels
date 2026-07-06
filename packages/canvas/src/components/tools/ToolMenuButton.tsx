import { Button, cn } from "@heroui/react"
import { type KeyboardEvent, useCallback, useEffect, useRef } from "react"
import { Shortcut } from "./ToolShortcut"
import type { ToolConfigItem } from "./types"

const MENU_CLOSE_DELAY_MS = 40

interface ToolMenuButtonProps {
  activeToolId?: string | null
  buttonIcon: ToolConfigItem["icon"]
  buttonLabel: string
  getToolLabel: (tool: ToolConfigItem) => string
  isActive: boolean
  isMenuOpen: boolean
  onButtonPress: () => void
  onMenuClose: () => void
  onMenuOpen: () => void
  onToolSelect: (toolId: string) => void
  tools: ToolConfigItem[]
}

export function ToolMenuButton({
  activeToolId = null,
  buttonIcon,
  buttonLabel,
  getToolLabel,
  isActive,
  isMenuOpen,
  onMenuClose,
  onMenuOpen,
  onButtonPress,
  onToolSelect,
  tools,
}: ToolMenuButtonProps) {
  const closeTimeoutRef = useRef<number | null>(null)

  const clearCloseTimeout = useCallback(() => {
    if (closeTimeoutRef.current === null) {
      return
    }

    window.clearTimeout(closeTimeoutRef.current)
    closeTimeoutRef.current = null
  }, [])

  const openMenu = useCallback(() => {
    clearCloseTimeout()
    onMenuOpen()
  }, [clearCloseTimeout, onMenuOpen])

  const closeMenu = useCallback(() => {
    clearCloseTimeout()
    onMenuClose()
  }, [clearCloseTimeout, onMenuClose])

  const scheduleClose = useCallback(() => {
    clearCloseTimeout()
    closeTimeoutRef.current = window.setTimeout(() => {
      onMenuClose()
      closeTimeoutRef.current = null
    }, MENU_CLOSE_DELAY_MS)
  }, [clearCloseTimeout, onMenuClose])

  useEffect(() => {
    return () => {
      clearCloseTimeout()
    }
  }, [clearCloseTimeout])

  const handleButtonPress = useCallback(() => {
    if (isActive) {
      if (!isMenuOpen) {
        openMenu()
      }
      return
    }

    onButtonPress()
    closeMenu()
  }, [closeMenu, isActive, isMenuOpen, onButtonPress, openMenu])

  const handleMenuAction = useCallback(
    (toolId: string) => {
      if (activeToolId === toolId) {
        openMenu()
        return
      }

      onToolSelect(toolId)
      closeMenu()
    },
    [activeToolId, closeMenu, onToolSelect, openMenu]
  )

  const handleButtonKeyDown = useCallback(
    (event: KeyboardEvent<HTMLButtonElement>) => {
      if (event.key === "ArrowDown" || event.key === "ArrowRight") {
        event.preventDefault()
        openMenu()
      }

      if (event.key === "Escape") {
        closeMenu()
      }
    },
    [closeMenu, openMenu]
  )

  return (
    <div
      className="relative"
      onMouseEnter={openMenu}
      onMouseLeave={scheduleClose}
    >
      <Button
        aria-expanded={isMenuOpen}
        aria-haspopup="menu"
        aria-label={buttonLabel}
        className="cursor-pointer select-none"
        isIconOnly
        onKeyDown={handleButtonKeyDown}
        onPress={handleButtonPress}
        variant={isActive ? "primary" : "ghost"}
      >
        {buttonIcon}
      </Button>

      {isMenuOpen ? (
        <>
          <div
            aria-hidden="true"
            className="absolute -inset-y-2 left-full w-1.5"
          />
          <div
            aria-label={buttonLabel}
            className="absolute top-1/2 left-[calc(100%+0.375rem)] z-20 flex min-w-44 -translate-y-1/2 flex-col gap-0.5 overflow-hidden rounded-lg border border-border-default bg-bg-elevated p-1 shadow-lg"
            role="menu"
          >
            {tools.map((tool) => {
              const label = getToolLabel(tool)
              const isSelected = activeToolId === tool.id

              return (
                <button
                  aria-checked={isSelected}
                  className={cn(
                    "flex w-full cursor-pointer items-center gap-2 rounded-lg px-2.5 py-2 text-left text-sm text-text-primary transition-colors hover:bg-bg-muted focus-visible:bg-bg-muted focus-visible:outline-none",
                    isSelected ? "bg-bg-muted" : undefined
                  )}
                  key={tool.id}
                  onClick={() => handleMenuAction(tool.id)}
                  onMouseDown={(event) => {
                    event.preventDefault()
                  }}
                  role="menuitemradio"
                  type="button"
                >
                  <span className="shrink-0">{tool.icon}</span>
                  <span className="flex-1">{label}</span>
                  {tool.shortcut ? <Shortcut shortcut={tool.shortcut} /> : null}
                </button>
              )
            })}
          </div>
        </>
      ) : null}
    </div>
  )
}
