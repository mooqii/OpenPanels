import { StrictMode, useEffect, useState } from "react"
import { createRoot } from "react-dom/client"
import { App } from "./App"
import {
  applyMyOpenPanelsTheme,
  detectMyOpenPanelsTheme,
  MyOpenPanelsI18nProvider,
  MyOpenPanelsThemeProvider,
  useMyOpenPanelsI18n,
} from "./canvas"
import { apiFetch } from "./lib/api"
import { transportKey, useMyOpenPanelsTransport } from "./lib/transport"
import "./styles.css"

applyMyOpenPanelsTheme(detectMyOpenPanelsTheme())

function AppBootstrap() {
  const transport = useMyOpenPanelsTransport()
  const { locale, t } = useMyOpenPanelsI18n()
  const [readyLocale, setReadyLocale] = useState<string | null>(null)
  const [localeError, setLocaleError] = useState<string | null>(null)

  useEffect(() => {
    if (!transport) return
    let cancelled = false
    setReadyLocale(null)
    setLocaleError(null)
    apiFetch(transport.apiBase, "/api/skills/preset-locale", {
      body: JSON.stringify({ locale }),
      headers: { "content-type": "application/json" },
      method: "PUT",
    })
      .then((response) => {
        if (!response.ok) throw new Error(`HTTP ${response.status}`)
        if (!cancelled) setReadyLocale(locale)
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setLocaleError(error instanceof Error ? error.message : String(error))
        }
      })
    return () => {
      cancelled = true
    }
  }, [locale, transport])

  if (!transport || readyLocale !== locale) {
    return (
      <main className="design-shell design-shell--status">
        <div className="op-boot-status">
          <div>
            {localeError
              ? t`Unable to load Preset Skills`
              : t`Loading Preset Skills`}
          </div>
          {localeError ? (
            <div className="op-boot-status__detail">{localeError}</div>
          ) : null}
        </div>
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
