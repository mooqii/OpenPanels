import { Button, Popover } from "@heroui/react"
import { CheckCircle2, Download, RefreshCw } from "lucide-react"
import type {
  MyOpenPanelsBuildInfo,
  MyOpenPanelsUpdateStatus,
} from "../../types"
import { formatLocalBuildTime } from "./trace-utils"

export function BuildVersionBadge({
  info,
  isChecking,
  onCheckUpdate,
  onUpdate,
  status,
}: {
  info: MyOpenPanelsBuildInfo
  isChecking: boolean
  onCheckUpdate: (options?: { refresh?: boolean }) => void
  onUpdate: () => void
  status: MyOpenPanelsUpdateStatus | null
}) {
  const localBuildTime = info.buildTime
    ? formatLocalBuildTime(info.buildTime)
    : null
  const label =
    info.channel === "development" && localBuildTime
      ? localBuildTime
      : info.label
  const hasUpdate = Boolean(status?.updateAvailable || status?.readyToInstall)
  const currentVersion = status?.currentVersion ?? info.version
  const latestVersion = status?.latestVersion ?? null
  const updateText = isChecking
    ? "正在检查更新"
    : hasUpdate
      ? `发现新版本 ${latestVersion ?? ""}`.trim()
      : status
        ? "当前已是最新版"
        : "点击检查更新"
  const updateDetail = status
    ? `当前 ${currentVersion}${latestVersion ? ` · 最新 ${latestVersion}` : ""}`
    : "会从 GitHub Release 获取最新状态"

  return (
    <Popover
      onOpenChange={(isOpen) => {
        if (isOpen) onCheckUpdate({ refresh: true })
      }}
    >
      <Button
        aria-label="查看版本与更新状态"
        className="op-build-badge"
        size="sm"
        variant="ghost"
      >
        {label}
      </Button>
      <Popover.Content placement="top end">
        <Popover.Dialog className="min-w-72">
          <div className="op-build-popover__status">
            <span
              className={`op-build-popover__icon ${
                isChecking
                  ? "op-build-popover__icon--checking"
                  : hasUpdate
                    ? "op-build-popover__icon--update"
                    : "op-build-popover__icon--current"
              }`}
            >
              {isChecking ? (
                <RefreshCw size={15} />
              ) : hasUpdate ? (
                <Download size={15} />
              ) : (
                <CheckCircle2 size={15} />
              )}
            </span>
            <div>
              <strong>{updateText}</strong>
              <span>{updateDetail}</span>
            </div>
          </div>
          {hasUpdate ? (
            <div className="op-build-popover__actions">
              <Button
                isDisabled={isChecking}
                onPress={onUpdate}
                size="sm"
                variant="primary"
              >
                立即更新
              </Button>
            </div>
          ) : null}
        </Popover.Dialog>
      </Popover.Content>
    </Popover>
  )
}
