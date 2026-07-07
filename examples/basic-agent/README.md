# Basic Agent Example

Shell-capable agents should first start the MyOpenPanels studio with the CLI,
open the returned `serverUrl`, and then use the project-backed CLI commands for
canvas state, selection, and image insertion.

```bash
openpanels-local studio start --project "$PWD" --format json
openpanels-local selection --project "$PWD" --format json
openpanels-local insert-image --project "$PWD" --image /tmp/result.png --placement right --format json
```

The local server still exposes HTTP APIs for the browser studio. Advanced
programmatic clients can use `@openpanels/sdk` against the `serverUrl` returned
by `openpanels-local studio start`.

```ts
import { createOpenPanelsClient } from "@openpanels/sdk"

const client = createOpenPanelsClient({ endpoint: "http://127.0.0.1:47321" })
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
