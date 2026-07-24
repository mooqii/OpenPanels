import {
  type Dispatch,
  type RefObject,
  type SetStateAction,
  useCallback,
} from "react"
import { replaceAppPanelState } from "../lib/api"
import type {
  AppState,
  ProjectTask,
  PublishingState,
  TypesettingState,
} from "../types"

export function usePanelStateSaves({
  appStateRef,
  setAppState,
}: {
  appStateRef: RefObject<AppState | null>
  setAppState: Dispatch<SetStateAction<AppState | null>>
}) {
  const handleTypesettingStateSaved = useCallback(
    (savedState: TypesettingState, savedRevision: number) => {
      setAppState((current) => {
        if (!current) return current
        const next = {
          ...current,
          panels: current.panels.map((snapshot) =>
            snapshot.panel.kind === "typesetting"
              ? {
                  ...replaceAppPanelState(snapshot, savedState),
                  revision: savedRevision,
                }
              : snapshot
          ),
          revision:
            current.activePanelKind === "typesetting"
              ? savedRevision
              : current.revision,
          state:
            current.activePanelKind === "typesetting"
              ? savedState
              : current.state,
        }
        appStateRef.current = next
        return next
      })
    },
    [appStateRef, setAppState]
  )

  const handlePublishingStateSaved = useCallback(
    (
      savedState: PublishingState,
      savedRevision: number,
      task?: ProjectTask
    ) => {
      setAppState((current) => {
        if (!current) return current
        const tasks = task
          ? [
              task,
              ...(current.tasks ?? []).filter((item) => item.id !== task.id),
            ]
          : current.tasks
        const next = {
          ...current,
          panels: current.panels.map((snapshot) =>
            snapshot.panel.kind === "publishing"
              ? {
                  ...replaceAppPanelState(snapshot, savedState),
                  revision: savedRevision,
                }
              : snapshot
          ),
          revision:
            current.activePanelKind === "publishing"
              ? savedRevision
              : current.revision,
          state:
            current.activePanelKind === "publishing"
              ? savedState
              : current.state,
          tasks,
        }
        appStateRef.current = next
        return next
      })
    },
    [appStateRef, setAppState]
  )

  return { handlePublishingStateSaved, handleTypesettingStateSaved }
}
