import type { UpdateAction } from "../hooks/use-studio-update"
import type {
  MyOpenPanelsBuildInfo,
  MyOpenPanelsTransport,
  MyOpenPanelsUpdateStatus,
} from "../types"
import { ModelGatewaySettingsDialog } from "./settings/ModelGatewaySettings"
import { AgentToggleButton, BuildVersionBadge } from "./trace/TracePanel"
import {
  type StudioRuntimeState,
  StudioRuntimeStatus,
} from "./update/StudioRuntimeStatus"
import { UpdatePrompt } from "./update/UpdatePrompt"

export function AppOverlays({
  buildInfo,
  isModelSettingsOpen,
  isTraceOpen,
  onCheckUpdate,
  onDismissUpdateError,
  onRefreshUpdate,
  onRetryConnect,
  onToggleAgentPanel,
  onUpdate,
  pendingTaskCount,
  runtimeState,
  setIsModelSettingsOpen,
  transport,
  updateAction,
  updateError,
  updateStatus,
}: {
  buildInfo?: MyOpenPanelsBuildInfo
  isModelSettingsOpen: boolean
  isTraceOpen: boolean
  onCheckUpdate: (options?: { refresh?: boolean }) => void
  onDismissUpdateError: () => void
  onRefreshUpdate: () => void
  onRetryConnect: () => void
  onToggleAgentPanel: () => void
  onUpdate: () => void
  pendingTaskCount: number
  runtimeState: StudioRuntimeState
  setIsModelSettingsOpen: (isOpen: boolean) => void
  transport: MyOpenPanelsTransport
  updateAction: UpdateAction
  updateError: string | null
  updateStatus: MyOpenPanelsUpdateStatus | null
}) {
  return (
    <>
      <div className="op-status-cluster">
        {!isTraceOpen && buildInfo ? (
          <BuildVersionBadge
            info={buildInfo}
            isChecking={updateAction === "checking"}
            onCheckUpdate={onCheckUpdate}
            onUpdate={onUpdate}
            status={updateStatus}
          />
        ) : null}
        <AgentToggleButton
          isOpen={isTraceOpen}
          onToggle={onToggleAgentPanel}
          pendingCount={pendingTaskCount}
        />
      </div>
      <ModelGatewaySettingsDialog
        isOpen={isModelSettingsOpen}
        onOpenChange={setIsModelSettingsOpen}
        transport={transport}
      />
      <UpdatePrompt
        action={updateAction}
        errorMessage={updateError}
        onDismissError={onDismissUpdateError}
        onRefresh={onRefreshUpdate}
        onRetryConnect={onRetryConnect}
        onUpdate={onUpdate}
        status={updateStatus}
      />
      {updateAction ? null : (
        <StudioRuntimeStatus onRetry={onRetryConnect} state={runtimeState} />
      )}
    </>
  )
}
