# OpenPanels v0.1 Engineering Spec

Status: Draft  
Audience: AI coding agents implementing the first usable OpenPanels version  
Scope: OpenPanels open-source local product only  
Out of scope: OpenPanelsCloud, accounts, billing, team sync, hosted AI services

## 1. Product Goal

OpenPanels v0.1 is a Codex-first, local, open-source agent panel system.

The first release must let Codex open a native panel workspace, send artifacts into panels, render an interactive canvas panel migrated from Moodbook, and persist all local state under the current project directory.

The v0.1 product is not only an SDK. It must be a complete runnable repository:

```bash
git clone <openpanels repo>
cd openpanels
pnpm install
pnpm dev
```

The initial user experience should be close to Cowart's product shape:

```txt
Codex
  -> OpenPanels MCP tools
  -> native widget / local studio
  -> OpenPanels runtime
  -> registered panels
  -> project-local .openpanels storage
```

## 2. Hard Decisions For v0.1

- OpenPanels v0.1 supports Codex only.
- The protocol can be designed cleanly, but no non-Codex adapter is required in v0.1.
- Local persistence uses a hidden `.openpanels/` directory in the active user project.
- Moodbook's `react-konva + zustand + editor/store/types/hooks/renderers` canvas is the first large migrated asset.
- The canvas is migrated into `packages/canvas/`.
- OpenPanelsCloud must not be implemented in this spec.
- Do not copy Moodbook auth, billing, database, routes, cloud resource library, subscriptions, or AI generation workflows.
- Do not make `OpenPanelsCloud` a fork or nested app inside OpenPanels.

## 3. Repository Shape

Create the OpenPanels repository as a pnpm monorepo.

```txt
OpenPanels/
  apps/
    local-studio/

  packages/
    protocol/
    core/
    runtime/
    react/
    sdk/
    canvas/
    local-server/
    local-storage/

  panels/
    image/
    diff/
    preview/
    files/

  mcp/
    server.mjs
    lib/

  skills/
    openpanels-open/
    openpanels-image/

  .codex-plugin/
    plugin.json

  docs/
    specs/
      openpanels-v0.1-spec.md

  examples/
    basic-agent/
    custom-panel/

  scripts/
  package.json
  pnpm-workspace.yaml
  tsconfig.json
  README.md
```

For v0.1, `panels/` can stay thin. The real canvas implementation lives in `packages/canvas`.

## 4. Package Responsibilities

### 4.1 `packages/protocol`

Defines stable serializable contracts shared by MCP tools, runtime, SDK, and panels.

Must contain:

- panel IDs
- session IDs
- artifact schemas
- panel event schemas
- runtime command schemas
- persisted snapshot schemas

Recommended dependencies:

- `zod`
- no React
- no filesystem access
- no Codex-specific imports

Initial exported types:

```ts
export type OpenPanelsSessionId = string
export type OpenPanelsPanelId = string
export type OpenPanelsArtifactId = string

export type OpenPanelsPanelKind =
  | "canvas"
  | "diff"
  | "preview"
  | "files"

export interface OpenPanelsSession {
  id: OpenPanelsSessionId
  title: string
  createdAt: string
  updatedAt: string
  panelIds: OpenPanelsPanelId[]
}

export interface OpenPanelsPanel {
  id: OpenPanelsPanelId
  sessionId: OpenPanelsSessionId
  kind: OpenPanelsPanelKind
  title: string
  createdAt: string
  updatedAt: string
  stateRef?: string
}

export type OpenPanelsArtifact =
  | ImageArtifact
  | CanvasArtifact
  | DiffArtifact
  | FileArtifact
  | PreviewArtifact
```

v0.1 must fully support only:

- `image`
- `canvas`

The other artifact kinds can have schemas but do not need complete UI behavior.

### 4.2 `packages/core`

Contains non-UI domain logic.

Must contain:

- ID creation helpers
- panel registry model
- artifact normalization
- runtime state helpers
- lightweight validation wrappers around `packages/protocol`

Must not contain:

- React
- Vite
- Codex MCP server code
- filesystem persistence

### 4.3 `packages/runtime`

Runs panel sessions and dispatches commands to registered panels.

Must contain:

- `OpenPanelsRuntime`
- session creation
- panel creation
- panel registry
- artifact insertion
- event subscription
- persistence integration interface

Core runtime shape:

```ts
export interface OpenPanelsRuntimeOptions {
  storage: OpenPanelsStorage
  registry: OpenPanelsPanelRegistry
}

export interface OpenPanelsRuntime {
  createSession(input: CreateSessionInput): Promise<OpenPanelsSession>
  openPanel(input: OpenPanelInput): Promise<OpenPanelsPanel>
  insertArtifact(input: InsertArtifactInput): Promise<OpenPanelsArtifact>
  getSession(sessionId: string): Promise<OpenPanelsSession | null>
  subscribe(listener: OpenPanelsRuntimeListener): () => void
}
```

### 4.4 `packages/react`

Provides reusable React rendering primitives for OpenPanels panels.

Must contain:

- `OpenPanelsProvider`
- `PanelHost`
- `PanelFrame`
- `PanelToolbar`
- `useOpenPanelsRuntime`
- `usePanelState`

Must not contain:

- local storage implementation
- Codex MCP tool logic
- Moodbook-specific application code

### 4.5 `packages/sdk`

Provides a small agent-facing client.

For v0.1, SDK can be a local TypeScript client used by examples and tests. Codex primarily enters through MCP tools.

Initial API:

```ts
const client = createOpenPanelsClient({ endpoint: "http://localhost:..." })

await client.openPanel({
  kind: "canvas",
  title: "Storyboard",
})

await client.insertArtifact({
  panelId,
  artifact: {
    kind: "image",
    assetRef: "assets/generated.png",
    mimeType: "image/png",
  },
})
```

### 4.6 `packages/local-server`

Runs the local OpenPanels service used by `apps/local-studio` and MCP tools.

Must contain:

- HTTP API for local studio and SDK
- runtime singleton per active project directory
- websocket or server-sent events for runtime updates
- project directory resolution

The first implementation may be simple and Vite-friendly. Do not overbuild production deployment.

### 4.7 `packages/local-storage`

Implements project-local `.openpanels/` persistence.

Must contain:

- session metadata read/write
- panel metadata read/write
- panel state read/write
- asset write/read helpers
- safe path handling

Storage root:

```txt
<project>/.openpanels/
```

Recommended layout:

```txt
.openpanels/
  sessions/
    <session-id>/
      session.json
      panels/
        <panel-id>/
          panel.json
          state.json
          assets/
            <asset-id>.<ext>
  index.json
```

All generated file and directory names must be sanitized. No tool may write outside `.openpanels/`.

### 4.8 `packages/canvas`

First-party OpenPanels canvas panel, migrated from Moodbook.

Must expose:

```ts
export function CanvasPanel(props: CanvasPanelProps): JSX.Element
export function createCanvasEditor(options?: CanvasEditorOptions): Editor
export type CanvasSnapshot = StoreSnapshot
export type CanvasAssetStore = AssetStore
```

The canvas package owns canvas editor internals. It must not depend on Moodbook app routes, auth, billing, database, cloud resource types, or Lingui catalogs.

## 5. Moodbook Canvas Migration

Source project:

```txt
/Users/mooqii/Code/OpenPanelsProject/Moodbook
```

Destination:

```txt
/Users/mooqii/Code/OpenPanelsProject/OpenPanels/packages/canvas
```

### 5.1 Migrate These Moodbook Files

Start with the complete canvas subtree:

```txt
Moodbook/src/canvas/
```

Important files include:

```txt
src/canvas/Canvas.tsx
src/canvas/EditorContext.tsx
src/canvas/editor.ts
src/canvas/store.ts
src/canvas/types/
src/canvas/hooks/
src/canvas/renderers/
src/canvas/components/
src/canvas/utils/
src/canvas/constants.ts
src/canvas/text-layout.ts
src/canvas/text-tool.ts
```

### 5.2 Do Not Migrate These Moodbook Systems

Do not migrate:

```txt
src/routes/
src/lib/auth*
src/middlewares/
src/shared/billing*
src/shared/subscriptions*
src/shared/orders*
src/shared/profile*
src/shared/design-projects*
src/shared/resources*
src/locales/
src/emails/
docker/
content/
```

If a canvas file imports one of these systems, replace that dependency with a canvas-local interface.

### 5.3 Required Canvas Refactor

The migrated canvas must become app-agnostic.

Replace Moodbook-specific assumptions with props or interfaces:

```ts
export interface CanvasPanelProps {
  snapshot?: CanvasSnapshot
  assetStore?: CanvasAssetStore
  width?: number
  height?: number
  readOnly?: boolean
  onSnapshotChange?: (snapshot: CanvasSnapshot) => void
  onAssetCreate?: (asset: CanvasAsset) => void
}
```

The package may keep the internal `Editor`, `CanvasStore`, `Shape`, `Asset`, and renderer architecture.

### 5.4 Canvas Dependencies

The canvas package may depend on:

```txt
react
react-dom
konva
react-konva
zustand
immer
lucide-react
perfect-freehand
```

Avoid dependencies on:

```txt
@tanstack/react-router
@tanstack/react-start
better-auth
kysely
stripe
@lingui/*
@heroui/react
```

If UI controls need styling, use package-local CSS first. Do not pull the entire Moodbook app shell into `packages/canvas`.

## 6. Local Studio

`apps/local-studio` is the open-source local UI for OpenPanels.

For v0.1 it must provide:

- session list or current session display
- panel host area
- ability to render canvas panel
- ability to render image artifact
- basic dev/test controls for inserting sample artifacts

It should be implemented with:

- Vite
- React
- TypeScript

It does not need:

- auth
- database
- billing
- cloud sync
- admin screens

## 7. Codex MCP Plugin

OpenPanels v0.1 must include a Codex plugin and MCP server.

Use Cowart as the product reference:

```txt
/Users/mooqii/Code/OpenPanelsProject/Cowart/.codex-plugin
/Users/mooqii/Code/OpenPanelsProject/Cowart/mcp
/Users/mooqii/Code/OpenPanelsProject/Cowart/skills
```

Do not copy Cowart names directly. Use OpenPanels names.

### 7.1 Required MCP Tools

Required tools:

```txt
render_openpanels_widget
get_openpanels_session
open_openpanels_panel
insert_openpanels_artifact
save_openpanels_panel_state
read_openpanels_panel_asset
write_openpanels_panel_asset
```

The first working path can be:

```txt
render_openpanels_widget
open_openpanels_panel(kind="canvas")
insert_openpanels_artifact(kind="image" or "canvas")
```

### 7.2 Widget Behavior

`render_openpanels_widget` opens the OpenPanels native widget for the active project.

The widget must know:

- active project directory
- `.openpanels/` storage root
- current session ID
- local server URL or embedded widget resource

### 7.3 Skills

Create at least:

```txt
skills/openpanels-open/SKILL.md
skills/openpanels-image/SKILL.md
```

The skills must instruct Codex agents to use MCP tools rather than manually editing `.openpanels/` files.

## 8. Panel Model

A panel is a registered renderer plus persisted state.

```ts
export interface PanelDefinition {
  kind: OpenPanelsPanelKind
  title: string
  canHandleArtifact: (artifact: OpenPanelsArtifact) => boolean
  createInitialState: () => unknown
}
```

React panel definitions can extend this:

```ts
export interface ReactPanelDefinition<TState = unknown> extends PanelDefinition {
  component: React.ComponentType<ReactPanelProps<TState>>
}
```

Panel state is owned by the panel but persisted by runtime storage.

For v0.1:

- canvas panel state is a canvas `StoreSnapshot`
- image artifacts are inserted into or associated with the canvas

## 9. Artifact Model

Artifacts are content units inserted by an agent or UI.

Initial v0.1 artifact types:

```ts
export interface ImageArtifact {
  id: string
  kind: "image"
  panelId?: string
  title?: string
  mimeType: string
  assetRef: string
  width?: number
  height?: number
  createdAt: string
}

export interface CanvasArtifact {
  id: string
  kind: "canvas"
  panelId?: string
  snapshot: unknown
  createdAt: string
}
```

`assetRef` must point to an asset managed by `.openpanels/`, not an arbitrary unsafe path.

## 10. Storage Safety Rules

All local storage code must follow these rules:

- Never write outside the resolved `.openpanels/` root.
- Reject path traversal.
- Sanitize user-provided filenames.
- Store JSON with stable indentation.
- Prefer content-addressed or generated IDs over raw filenames.
- Keep assets inside the owning panel directory unless there is a deliberate shared asset store.
- Do not silently overwrite assets. Generate unique names.

## 11. Development Milestones

### Milestone 1: Monorepo Skeleton

Deliver:

- `pnpm-workspace.yaml`
- root `package.json`
- root `tsconfig.json`
- `apps/local-studio`
- empty package shells for protocol/core/runtime/react/sdk/local-server/local-storage/canvas
- root scripts:

```json
{
  "dev": "pnpm --filter @openpanels/local-studio dev",
  "build": "pnpm -r build",
  "typecheck": "pnpm -r typecheck",
  "test": "pnpm -r test"
}
```

Acceptance:

```bash
pnpm install
pnpm dev
```

opens a local React app.

### Milestone 2: Protocol And Runtime

Deliver:

- protocol schemas
- runtime session creation
- runtime panel creation
- artifact insertion
- in-memory storage adapter for tests

Acceptance:

- unit tests can create a session
- unit tests can open a canvas panel
- unit tests can insert image and canvas artifacts

### Milestone 3: Local Storage

Deliver:

- `.openpanels/` storage implementation
- session/panel/state JSON persistence
- asset write/read

Acceptance:

- inserting an artifact creates files under `.openpanels/`
- restarting local server can read the saved session

### Milestone 4: React Host And Local Studio

Deliver:

- `OpenPanelsProvider`
- `PanelHost`
- local studio renders registered panels
- canvas-first controls for image upload, image placement, and canvas editing

Acceptance:

- user can open local studio
- user can create a canvas panel
- user can insert image artifacts and edit them on the canvas

### Milestone 5: Moodbook Canvas Migration

Deliver:

- migrated `packages/canvas`
- canvas panel renders in local studio
- canvas snapshot can be saved and reloaded
- basic selection/drawing/image/text behavior remains working

Acceptance:

- `CanvasPanel` mounts without Moodbook app dependencies
- package typechecks independently
- canvas state round-trips through OpenPanels runtime storage

### Milestone 6: Codex Plugin And MCP Tools

Deliver:

- `.codex-plugin/plugin.json`
- `mcp/server.mjs`
- required MCP tools
- skills

Acceptance:

- Codex can open OpenPanels widget
- Codex can open a canvas panel
- Codex can insert an image asset into the canvas

## 12. Testing Strategy

Use focused tests. Do not attempt full end-to-end coverage before the product skeleton works.

Required early tests:

- protocol schema parsing
- runtime session and panel creation
- artifact insertion
- local storage path safety
- local storage JSON round-trip
- canvas package typecheck
- canvas snapshot round-trip

Recommended test tools:

- Vitest
- React Testing Library where needed
- Playwright only after local studio is stable

## 13. First Implementation Order For Agents

When an AI agent starts implementing this spec, follow this order:

1. Create monorepo skeleton.
2. Implement `packages/protocol`.
3. Implement `packages/runtime` with in-memory storage.
4. Implement `packages/local-storage`.
5. Implement `apps/local-studio` as a canvas-first design workspace.
6. Migrate Moodbook canvas into `packages/canvas`.
7. Register canvas as the first real OpenPanels panel.
8. Add MCP server and Codex plugin.
9. Add skills.
10. Update README with clone/install/dev/plugin instructions.

Do not start from Cloud. Do not start from billing/auth. Do not start by publishing npm packages.

## 14. Definition Of Done For v0.1

OpenPanels v0.1 is done when:

- repository installs with pnpm
- local studio runs
- Codex can open OpenPanels
- Codex can create or show a session
- Codex can open a canvas panel
- canvas panel uses the migrated Moodbook canvas package
- image artifacts can be inserted into the canvas
- state persists under `.openpanels/`
- restart does not lose saved sessions
- README documents the local workflow

## 15. Non-Goals

Do not implement in v0.1:

- OpenPanelsCloud
- user accounts
- organizations
- permissions
- billing
- remote sync
- cloud-hosted sessions
- multi-user collaboration
- npm publishing pipeline
- marketplace for third-party panels
- full diff review workflow
- production deployment

## 16. Guiding Principle

OpenPanels v0.1 should prove one thing:

An AI agent can reliably send useful artifacts into a persistent, interactive local panel system, with Moodbook's canvas becoming the first serious panel inside that system.
