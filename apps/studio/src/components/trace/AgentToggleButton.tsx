import { Button } from "@heroui/react"
import { Activity } from "lucide-react"
import { formatTaskCount } from "./trace-utils"

export function AgentToggleButton({
  isOpen,
  pendingCount,
  onToggle,
}: {
  isOpen: boolean
  pendingCount: number
  onToggle: () => void
}) {
  return (
    <Button
      aria-expanded={isOpen}
      aria-label={isOpen ? "折叠 Agent 面板" : "展开 Agent 面板"}
      className={`op-trace-toggle ${isOpen ? "op-trace-toggle--active" : ""}`}
      isIconOnly
      onPress={onToggle}
      size="sm"
      variant={isOpen ? "secondary" : "ghost"}
    >
      <Activity size={14} />
      {pendingCount > 0 ? (
        <span className="op-trace-toggle__dot">
          {formatTaskCount(pendingCount)}
        </span>
      ) : null}
    </Button>
  )
}
