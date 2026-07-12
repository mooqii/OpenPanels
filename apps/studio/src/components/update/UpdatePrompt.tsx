import { Button } from "@heroui/react"
import { AlertTriangle, LoaderCircle, RefreshCw, X } from "lucide-react"
import { useState } from "react"
import type { MyOpenPanelsUpdateStatus } from "../../types"

export type UpdateAction =
  | "checking"
  | "downloading"
  | "installing"
  | "restarting"
  | "failed"
  | null

export function UpdatePrompt({
  action,
  errorMessage,
  onDismissError,
  onRefresh,
  onRetryConnect,
  onUpdate,
  status,
}: {
  action: UpdateAction
  errorMessage: string | null
  onDismissError: () => void
  onRefresh: () => void
  onRetryConnect: () => void
  onUpdate: () => void
  status: MyOpenPanelsUpdateStatus | null
}) {
  const latest = status?.latestVersion ?? "new"
  const visible = Boolean(status?.updateAvailable || status?.readyToInstall)
  const [dismissedVersion, setDismissedVersion] = useState<string | null>(null)
  const recoveryCommand =
    '请先运行 myopenpanels update install --format json 安装最新的 MyOpenPanels CLI；安装成功后，再运行 myopenpanels studio start --local-only --project-dir "$PWD" --format json 重新启动 Studio。'

  if (
    action === "installing" ||
    action === "restarting" ||
    action === "failed"
  ) {
    return (
      <div
        className="op-update-overlay"
        role={action === "failed" ? "alert" : "status"}
      >
        <div className="op-update-overlay__panel">
          <span
            className={`op-update-overlay__icon ${
              action === "failed"
                ? "op-update-overlay__icon--failed"
                : "op-update-overlay__icon--busy"
            }`}
          >
            {action === "failed" ? (
              <AlertTriangle size={18} strokeWidth={1.8} />
            ) : (
              <LoaderCircle size={18} strokeWidth={1.8} />
            )}
          </span>
          <div className="op-update-overlay__copy">
            <strong>
              {action === "failed"
                ? "更新没有自动恢复"
                : action === "restarting"
                  ? "正在切换到新版 Studio"
                  : "正在安装 MyOpenPanels 更新"}
            </strong>
            <span>
              {action === "failed"
                ? (errorMessage ??
                  "请让 agent 重新打开 MyOpenPanels 面板，或稍后重新连接。")
                : action === "restarting"
                  ? "完成后会自动恢复当前面板。"
                  : "请保持此页面打开，安装完成后会自动重启。"}
            </span>
          </div>
          {action === "failed" ? (
            <div className="op-update-overlay__actions">
              <Button onPress={onRetryConnect} size="sm" variant="ghost">
                重新连接
              </Button>
              <Button
                onPress={() => navigator.clipboard?.writeText(recoveryCommand)}
                size="sm"
                variant="ghost"
              >
                复制恢复指令
              </Button>
              <Button onPress={onDismissError} size="sm">
                关闭
              </Button>
            </div>
          ) : null}
        </div>
      </div>
    )
  }

  if (!visible || dismissedVersion === latest) return null

  const busy = action !== null
  const primaryLabel = action === "downloading" ? "正在下载" : "立即更新"

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
        <Button
          aria-label="关闭更新提示"
          className="op-update-prompt__dismiss"
          isIconOnly
          onPress={() => setDismissedVersion(latest)}
          size="sm"
          variant="ghost"
        >
          <X size={15} strokeWidth={1.8} />
        </Button>
      </div>
    </div>
  )
}
