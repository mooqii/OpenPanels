import { createLocalOpenPanelsServer } from "./index"

const projectDir = process.env.OPENPANELS_PROJECT_DIR ?? process.cwd()
const port = Number(process.env.OPENPANELS_PORT ?? 47_321)

const server = createLocalOpenPanelsServer({ projectDir })
server.listen(port, () => {
  console.log(`OpenPanels local server listening on http://localhost:${port}`)
})
