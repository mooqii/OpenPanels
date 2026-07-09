import {
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import type { CanvasSelectionSnapshot } from "./Canvas"
import { Canvas } from "./Canvas"
import { CanvasMenu } from "./components/CanvasMenu"
import { DEFAULT_TOOLBAR_CONFIG } from "./components/tools/default-config"
import { Toolbar } from "./components/tools/Toolbar"
import { ZoomControl } from "./components/ZoomControl"
import { EditorProvider } from "./EditorContext"
import { Editor, type EditorOptions } from "./editor"
import { usePreventBrowserZoom } from "./hooks/use-prevent-browser-zoom"
import { useOpenPanelsI18n } from "./i18n"
import { createCanvasStore } from "./store"
import { type AssetStore, DataUrlAssetStore } from "./types/assets"
import type { StoreSnapshot } from "./types/records"

export type { CanvasSelectionSnapshot } from "./Canvas"
export { Canvas } from "./Canvas"
export { CanvasMenu } from "./components/CanvasMenu"
export { Editor } from "./editor"
export {
  DEFAULT_OPENPANELS_LOCALE,
  detectOpenPanelsLocale,
  OPENPANELS_LOCALE_COOKIE,
  OPENPANELS_LOCALE_LABELS,
  OpenPanelsI18nProvider,
  type OpenPanelsLocale,
  translateOpenPanelsMessage,
  useOpenPanelsI18n,
} from "./i18n"
export { createCanvasStore } from "./store"
export {
  applyOpenPanelsTheme,
  DEFAULT_OPENPANELS_THEME,
  detectOpenPanelsTheme,
  OPENPANELS_THEME_COOKIE,
  type OpenPanelsTheme,
  OpenPanelsThemeProvider,
  useOpenPanelsTheme,
} from "./theme"
export { type Asset, type AssetStore, DataUrlAssetStore } from "./types/assets"
export type { StoreSnapshot } from "./types/records"

export interface CanvasPanelProps {
  assetStore?: AssetStore
  height?: number | string
  initialSnapshot?: StoreSnapshot
  onSelectionChange?: (selection: CanvasSelectionSnapshot) => void
  onSnapshotChange?: (snapshot: StoreSnapshot) => void
  readOnly?: boolean
  snapshot?: StoreSnapshot
  snapshotVersion?: number
  title?: string
  titleChromeContent?: ReactNode
  titleContent?: ReactNode
  width?: number | string
}

export function createCanvasEditor(options: EditorOptions = {}) {
  return new Editor(options)
}

export function createEmptyCanvasSnapshot(): StoreSnapshot {
  const store = createCanvasStore()
  return store.getState().getSnapshot()
}

export function CanvasPanel({
  assetStore,
  height = "100%",
  initialSnapshot,
  snapshot,
  snapshotVersion,
  title,
  titleChromeContent,
  titleContent,
  width = "100%",
  onSnapshotChange,
  onSelectionChange,
}: CanvasPanelProps) {
  const { t } = useOpenPanelsI18n()
  const displayTitle = titleContent ?? title ?? t`Untitled`
  const effectiveAssetStore = useMemo(
    () => assetStore ?? new DataUrlAssetStore(),
    [assetStore]
  )
  const initialSnapshotRef = useRef(snapshot ?? initialSnapshot)
  const loadedSnapshotVersionRef = useRef<number | null>(null)
  const cameraListenerCleanupRef = useRef<(() => void) | null>(null)
  const editor = useMemo(
    () =>
      new Editor({
        assetStore: effectiveAssetStore,
      }),
    [effectiveAssetStore]
  )
  const [dimensions, setDimensions] = useState({ width: 800, height: 600 })
  const containerRef = useRef<HTMLDivElement | null>(null)

  usePreventBrowserZoom(editor)

  useEffect(() => {
    if (snapshotVersion == null && initialSnapshotRef.current) {
      editor.loadSnapshot(initialSnapshotRef.current)
    }
  }, [editor, snapshotVersion])

  useEffect(() => {
    if (snapshotVersion == null || !snapshot) return
    if (loadedSnapshotVersionRef.current === snapshotVersion) return
    editor.loadSnapshot(snapshot)
    loadedSnapshotVersionRef.current = snapshotVersion
  }, [editor, snapshot, snapshotVersion])

  useEffect(() => {
    return editor.listen(() => {
      onSnapshotChange?.(editor.getSnapshot())
    })
  }, [editor, onSnapshotChange])

  const handleStageReady = useCallback(() => {
    cameraListenerCleanupRef.current?.()
    cameraListenerCleanupRef.current = null

    const stage = editor.stage
    if (!stage) return

    let frame = 0
    const syncCamera = () => {
      if (frame) {
        window.cancelAnimationFrame(frame)
      }
      frame = window.requestAnimationFrame(() => {
        frame = 0
        editor.syncCameraFromStage()
        onSnapshotChange?.(editor.getSnapshot())
      })
    }

    stage.on("xChange yChange scaleXChange scaleYChange", syncCamera)
    cameraListenerCleanupRef.current = () => {
      stage.off("xChange yChange scaleXChange scaleYChange", syncCamera)
      if (frame) {
        window.cancelAnimationFrame(frame)
      }
    }
  }, [editor, onSnapshotChange])

  useEffect(() => {
    return () => {
      cameraListenerCleanupRef.current?.()
      cameraListenerCleanupRef.current = null
    }
  }, [])

  useEffect(() => {
    const node = containerRef.current
    if (!node) return
    const observer = new ResizeObserver((entries) => {
      const rect = entries[0]?.contentRect
      if (!rect) return
      setDimensions({
        width: Math.max(1, rect.width),
        height: Math.max(1, rect.height),
      })
    })
    observer.observe(node)
    return () => observer.disconnect()
  }, [])

  return (
    <div
      className="op-canvas-panel"
      ref={containerRef}
      style={{ height, width }}
    >
      <EditorProvider editor={editor} toolbarConfig={DEFAULT_TOOLBAR_CONFIG}>
        <div className="op-canvas-title">
          {titleChromeContent ?? (
            <>
              <CanvasMenu />
              {typeof displayTitle === "string" ? (
                <span>{displayTitle}</span>
              ) : (
                displayTitle
              )}
            </>
          )}
        </div>
        <Canvas
          allowImagePaste
          height={dimensions.height}
          onSelectionChange={onSelectionChange}
          onStageReady={handleStageReady}
          width={dimensions.width}
        >
          <Toolbar />
          <ZoomControl />
        </Canvas>
      </EditorProvider>
    </div>
  )
}
