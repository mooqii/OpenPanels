import { useCallback, useState } from "react"
import { useToolbarConfig } from "../../EditorContext"
import { ToolButton } from "./ToolButton"
import { ToolGroup } from "./ToolGroup"
import type { ToolbarProps } from "./types"
import { isToolGroup } from "./types"

export function Toolbar({ children }: ToolbarProps) {
  const tools = useToolbarConfig()
  const [openMenuId, setOpenMenuId] = useState<string | null>(null)

  const handleMenuOpen = useCallback((menuId: string) => {
    setOpenMenuId(menuId)
  }, [])

  const handleMenuClose = useCallback((menuId: string) => {
    setOpenMenuId((currentMenuId) =>
      currentMenuId === menuId ? null : currentMenuId
    )
  }, [])

  return (
    <div
      className="fixed top-1/2 flex -translate-y-1/2 flex-col items-center gap-2 rounded-full bg-canvas-toolbar p-1.5 shadow backdrop-blur-lg"
      style={{
        left: "calc(var(--main-layout-sidebar-offset, var(--home-main-offset, 0px)) + 0.5rem)",
      }}
    >
      {tools.map((config) => {
        if (isToolGroup(config)) {
          const menuId = `group:${config.group}`

          return (
            <ToolGroup
              group={config}
              isMenuOpen={openMenuId === menuId}
              key={config.group}
              onMenuClose={() => handleMenuClose(menuId)}
              onMenuOpen={() => handleMenuOpen(menuId)}
            />
          )
        }
        const menuId = `tool:${config.id}`

        return (
          <ToolButton
            isMenuOpen={openMenuId === menuId}
            key={config.id}
            onMenuClose={() => handleMenuClose(menuId)}
            onMenuOpen={() => handleMenuOpen(menuId)}
            tool={config}
          />
        )
      })}

      {children}
    </div>
  )
}
