import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { App } from "./App"
import {
  applyOpenPanelsTheme,
  detectOpenPanelsTheme,
  OpenPanelsI18nProvider,
  OpenPanelsThemeProvider,
} from "./canvas"
import { transportKey, useOpenPanelsTransport } from "./lib/transport"
import "./styles.css"

applyOpenPanelsTheme(detectOpenPanelsTheme())

function AppBootstrap() {
  const transport = useOpenPanelsTransport()

  if (!transport) {
    return (
      <main className="design-shell design-shell--status">
        <div className="op-boot-status">Loading canvas</div>
      </main>
    )
  }

  return <App key={transportKey(transport)} transport={transport} />
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <OpenPanelsI18nProvider>
      <OpenPanelsThemeProvider>
        <AppBootstrap />
      </OpenPanelsThemeProvider>
    </OpenPanelsI18nProvider>
  </StrictMode>
)
