export type MyOpenPanelsPanelKind = "wiki" | "canvas"

export interface MyOpenPanelsProject {
  createdAt: string
  id: string
  panelIds: string[]
  title: string
  updatedAt: string
}

export interface MyOpenPanelsPanel {
  createdAt: string
  id: string
  kind: MyOpenPanelsPanelKind
  projectId: string
  stateRef?: string
  title: string
  updatedAt: string
}

export type MyOpenPanelsArtifact =
  | {
      assetRef: string
      createdAt: string
      height?: number
      id: string
      kind: "image"
      mimeType: string
      panelId?: string
      title?: string
      width?: number
    }
  | {
      createdAt: string
      id: string
      kind: "canvas"
      panelId?: string
      snapshot: unknown
      title?: string
    }
  | {
      createdAt: string
      diff: string
      id: string
      kind: "diff"
      panelId?: string
      title?: string
    }
  | {
      createdAt: string
      id: string
      kind: "file"
      mimeType?: string
      panelId?: string
      path: string
      title?: string
    }
  | {
      createdAt: string
      id: string
      kind: "preview"
      panelId?: string
      title?: string
      url: string
    }
