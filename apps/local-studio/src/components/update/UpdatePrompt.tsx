import { Button } from "@heroui/react"
import { RefreshCw } from "lucide-react"
import type { OpenPanelsUpdateStatus } from "../../types"

export function UpdatePrompt({
  action,
  onRefresh,
  onUpdate,
  status,
}: {
  action: "checking" | "downloading" | "installing" | null
  onRefresh: () => void
  onUpdate: () => void
  status: OpenPanelsUpdateStatus | null
}) {
  const visible = Boolean(status?.updateAvailable || status?.readyToInstall)
  if (!visible) return null

  const latest = status?.latestVersion ?? "new"
  const busy = action !== null
  const primaryLabel =
    action === "downloading"
      ? "正在下载"
      : action === "installing"
        ? "正在更新"
        : "立即更新"

  return (
    <div className="op-update-prompt">
      <div className="op-update-prompt__copy">
        <strong>有新版可更新</strong>
        <span>最新版本 {latest}</span>
      </div>
      <div className="op-update-prompt__actions">
        <Button
          aria-label="重新检查更新"
          isDisabled={busy}
          isIconOnly
          onPress={onRefresh}
          size="sm"
          variant="ghost"
        >
          <RefreshCw size={15} strokeWidth={1.8} />
        </Button>
        <Button
          className="op-update-prompt__primary"
          isDisabled={busy}
          onPress={onUpdate}
          size="sm"
        >
          {primaryLabel}
        </Button>
      </div>
    </div>
  )
}
