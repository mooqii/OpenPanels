import type {
  OpenPanelsArtifact,
  OpenPanelsPanel,
  OpenPanelsPanelKind,
} from "@openpanels/protocol"
import type { OpenPanelsRuntime } from "@openpanels/runtime"
import {
  createContext,
  type PropsWithChildren,
  type ReactNode,
  useContext,
} from "react"

export interface ReactPanelProps<TState = unknown> {
  artifacts: OpenPanelsArtifact[]
  onStateChange: (state: TState) => void
  panel: OpenPanelsPanel
  state: TState
}

export interface ReactPanelDefinition<TState = unknown> {
  component: (props: ReactPanelProps<TState>) => ReactNode
  kind: OpenPanelsPanelKind
}

export interface OpenPanelsReactContextValue {
  panels: Map<OpenPanelsPanelKind, ReactPanelDefinition>
  runtime: OpenPanelsRuntime
}

const OpenPanelsContext = createContext<OpenPanelsReactContextValue | null>(
  null
)

export function OpenPanelsProvider({
  runtime,
  panels,
  children,
}: PropsWithChildren<{
  runtime: OpenPanelsRuntime
  panels: ReactPanelDefinition[]
}>) {
  return (
    <OpenPanelsContext.Provider
      value={{
        runtime,
        panels: new Map(panels.map((panel) => [panel.kind, panel])),
      }}
    >
      {children}
    </OpenPanelsContext.Provider>
  )
}

export function useOpenPanelsRuntime(): OpenPanelsRuntime {
  const value = useContext(OpenPanelsContext)
  if (!value) throw new Error("OpenPanelsProvider is missing")
  return value.runtime
}

export function PanelFrame({
  title,
  children,
}: PropsWithChildren<{ title: string }>) {
  return (
    <article className="op-panel-frame">
      <header className="op-panel-frame__header">
        <h2>{title}</h2>
      </header>
      <div className="op-panel-frame__body">{children}</div>
    </article>
  )
}

export function PanelHost<TState = unknown>({
  panel,
  state,
  artifacts,
  onStateChange,
}: {
  panel: OpenPanelsPanel
  state: TState
  artifacts: OpenPanelsArtifact[]
  onStateChange: (state: TState) => void
}) {
  const value = useContext(OpenPanelsContext)
  if (!value) throw new Error("OpenPanelsProvider is missing")
  const definition = value.panels.get(panel.kind)
  if (!definition) {
    return (
      <PanelFrame title={panel.title}>
        <p>Unsupported panel: {panel.kind}</p>
      </PanelFrame>
    )
  }
  const Component = definition.component as (
    props: ReactPanelProps<TState>
  ) => ReactNode
  return (
    <PanelFrame title={panel.title}>
      {Component({ panel, state, artifacts, onStateChange })}
    </PanelFrame>
  )
}
