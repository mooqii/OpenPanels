import { useCallback, useEffect, useState } from "react"
import type { StudioRuntimeState } from "../components/update/StudioRuntimeStatus"
import {
  fetchUpdateStatus,
  isNotFoundError,
  requestUpdateDownload,
  requestUpdateInstallRestart,
} from "../lib/api"
import type { MyOpenPanelsTransport, MyOpenPanelsUpdateStatus } from "../types"

export type UpdateAction =
  | "checking"
  | "downloading"
  | "installing"
  | "restarting"
  | "failed"
  | null

export function useStudioUpdate(
  transport: MyOpenPanelsTransport,
  setRuntimeState: (state: StudioRuntimeState) => void
) {
  const [updateStatus, setUpdateStatus] =
    useState<MyOpenPanelsUpdateStatus | null>(null)
  const [updateAction, setUpdateAction] = useState<UpdateAction>(null)
  const [updateError, setUpdateError] = useState<string | null>(null)

  const refreshUpdateStatus = useCallback(
    async (options?: { refresh?: boolean }) => {
      setUpdateAction("checking")
      setUpdateError(null)
      try {
        const status = await fetchUpdateStatus(transport, options)
        setUpdateStatus(status)
      } catch (error) {
        if (!isNotFoundError(error)) {
          console.error("Failed to check MyOpenPanels update status", error)
        }
      } finally {
        setUpdateAction((current) => (current === "checking" ? null : current))
      }
    },
    [transport]
  )

  useEffect(() => {
    refreshUpdateStatus()
  }, [refreshUpdateStatus])

  const downloadUpdate = useCallback(async () => {
    setUpdateAction("downloading")
    setUpdateError(null)
    try {
      const status = await requestUpdateDownload(transport)
      setUpdateAction((current) => (current === "downloading" ? null : current))
      return status
    } catch (error) {
      console.error("Failed to download MyOpenPanels update", error)
      setUpdateError(
        "更新下载失败。请稍后重试，或让 agent 重新打开 MyOpenPanels 面板。"
      )
      setUpdateAction("failed")
      return null
    }
  }, [transport])

  const installAndRestartUpdate = useCallback(async () => {
    setUpdateAction("installing")
    setUpdateError(null)
    try {
      const result = await requestUpdateInstallRestart(transport)
      if (!result.restarting) {
        setUpdateAction(null)
        await refreshUpdateStatus({ refresh: true })
        return
      }
      setUpdateAction("restarting")
      window.dispatchEvent(new Event("myopenpanels:runtime-check"))
    } catch (error) {
      console.error("Failed to install MyOpenPanels update", error)
      setUpdateError(
        error instanceof Error && error.message
          ? error.message
          : "更新安装失败。请稍后重试，或让 agent 重新打开 MyOpenPanels 面板。"
      )
      setUpdateAction("failed")
    }
  }, [refreshUpdateStatus, transport])

  const retryUpdateReconnect = useCallback(() => {
    setUpdateAction("restarting")
    setUpdateError(null)
    setRuntimeState("reconnecting")
    window.dispatchEvent(new Event("myopenpanels:runtime-check"))
  }, [setRuntimeState])

  const dismissUpdateError = useCallback(() => {
    setUpdateAction(null)
    setUpdateError(null)
  }, [])

  const updateNow = useCallback(async () => {
    if (!(updateStatus?.updateAvailable || updateStatus?.readyToInstall)) return
    if (updateAction && updateAction !== "failed") return
    if (updateStatus.downloaded || updateStatus.readyToInstall) {
      installAndRestartUpdate()
      return
    }
    const status = await downloadUpdate()
    if (!(status?.downloaded || status?.readyToInstall)) return
    setUpdateStatus(status)
    installAndRestartUpdate()
  }, [downloadUpdate, installAndRestartUpdate, updateAction, updateStatus])

  const checkUpdateFromBadge = useCallback(
    (options?: { refresh?: boolean }) => {
      if (!updateAction) refreshUpdateStatus(options)
    },
    [refreshUpdateStatus, updateAction]
  )

  const refreshUpdateNow = useCallback(() => {
    refreshUpdateStatus({ refresh: true })
  }, [refreshUpdateStatus])

  return {
    checkUpdateFromBadge,
    dismissUpdateError,
    refreshUpdateNow,
    retryUpdateReconnect,
    setUpdateAction,
    setUpdateError,
    updateAction,
    updateError,
    updateNow,
    updateStatus,
  }
}
