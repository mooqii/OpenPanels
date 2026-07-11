import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { App } from "./App"
import {
  applyMyOpenPanelsTheme,
  detectMyOpenPanelsTheme,
  MyOpenPanelsI18nProvider,
  MyOpenPanelsThemeProvider,
} from "./canvas"
import { transportKey, useMyOpenPanelsTransport } from "./lib/transport"
import "./styles.css"

applyMyOpenPanelsTheme(detectMyOpenPanelsTheme())

function AppBootstrap() {
  const transport = useMyOpenPanelsTransport()

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
    <MyOpenPanelsI18nProvider>
      <MyOpenPanelsThemeProvider>
        <AppBootstrap />
      </MyOpenPanelsThemeProvider>
    </MyOpenPanelsI18nProvider>
  </StrictMode>
)
