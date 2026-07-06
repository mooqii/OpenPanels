# Basic Agent Example

Generic MCP agents should first call `start_openpanels_studio`, open the
returned `serverUrl`, and then use the canvas/asset tools exposed by the MCP
server.

Use `@openpanels/sdk` to create a local session and insert artifacts through the local server.

```ts
import { createOpenPanelsClient } from "@openpanels/sdk"

const client = createOpenPanelsClient({ endpoint: "http://localhost:47321" })
const session = await client.createSession({ title: "Agent run" })
const panel = await client.openPanel({ sessionId: session.id, kind: "canvas" })

await client.insertArtifact({
  sessionId: session.id,
  panelId: panel.id,
  artifact: {
    kind: "image",
    assetRef: "assets/result.png",
    mimeType: "image/png",
  },
})
```
