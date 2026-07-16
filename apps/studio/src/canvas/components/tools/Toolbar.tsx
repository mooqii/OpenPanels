import { useToolbarConfig } from "../../EditorContext"
import { ToolButton } from "./ToolButton"
import { ToolGroup } from "./ToolGroup"
import type { ToolbarProps } from "./types"
import { isToolGroup } from "./types"

export function Toolbar({ children }: ToolbarProps) {
  const tools = useToolbarConfig()

  return (
    <div
      className="fixed top-1/2 flex -translate-y-1/2 flex-col items-center gap-2 rounded-full bg-canvas-toolbar p-1.5 shadow backdrop-blur-lg"
      style={{
        left: "calc(var(--main-layout-sidebar-offset, var(--home-main-offset, 0px)) + 0.5rem)",
      }}
    >
      {tools.map((config) => {
        if (isToolGroup(config)) {
          return <ToolGroup group={config} key={config.group} />
        }
        return <ToolButton key={config.id} tool={config} />
      })}

      {children}
    </div>
  )
}
