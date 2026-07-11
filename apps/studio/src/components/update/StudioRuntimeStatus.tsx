import { Button } from "@heroui/react"
import { AlertTriangle, LoaderCircle, RefreshCw } from "lucide-react"

export type StudioRuntimeState =
  | "connected"
  | "reconnecting"
  | "switching"
  | "failed"

export function StudioRuntimeStatus({
  onRetry,
  state,
}: {
  onRetry: () => void
  state: StudioRuntimeState
}) {
  if (state === "connected") return null
  const failed = state === "failed"

  return (
    <div className="op-update-overlay" role={failed ? "alert" : "status"}>
      <div className="op-update-overlay__panel">
        <span
          className={`op-update-overlay__icon ${
            failed
              ? "op-update-overlay__icon--failed"
              : "op-update-overlay__icon--busy"
          }`}
        >
          {failed ? (
            <AlertTriangle size={18} strokeWidth={1.8} />
          ) : (
            <LoaderCircle size={18} strokeWidth={1.8} />
          )}
        </span>
        <div className="op-update-overlay__copy">
          <strong>
            {failed
              ? "Studio 没有自动恢复"
              : state === "switching"
                ? "正在切换到新版 Studio"
                : "正在重新连接 Studio"}
          </strong>
          <span>
            {failed
              ? "请确认服务已经启动，然后重新连接。"
              : "当前页面会保留，服务恢复后将自动继续。"}
          </span>
        </div>
        {failed ? (
          <div className="op-update-overlay__actions">
            <Button onPress={onRetry} size="sm">
              <RefreshCw size={14} strokeWidth={1.8} />
              重新连接
            </Button>
          </div>
        ) : null}
      </div>
    </div>
  )
}
