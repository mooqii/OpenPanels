import { useEffect, useState } from "react"
import type { MyOpenPanelsTransport } from "../types"

type MyOpenPanelsHostWindow = Window &
  typeof globalThis & {
    __MYOPENPANELS_API_BASE__?: string
    openai?: {
      rawToolResult?: {
        structuredContent?: {
          serverUrl?: string
        }
      }
      toolOutput?: {
        serverUrl?: string
      }
    }
  }

function localHttpOrigin(): string | null {
  if (window.location.protocol === "http:") {
    return window.location.origin
  }
  return null
}

function hostServerUrl(): string | null {
  const hostWindow = window as MyOpenPanelsHostWindow
  return (
    hostWindow.__MYOPENPANELS_API_BASE__ ??
    hostWindow.openai?.toolOutput?.serverUrl ??
    hostWindow.openai?.rawToolResult?.structuredContent?.serverUrl ??
    null
  )
}

function currentTransport(): MyOpenPanelsTransport | null {
  const localOrigin = localHttpOrigin()
  if (localOrigin) return { apiBase: localOrigin, kind: "http" }

  const serverUrl = hostServerUrl()
  if (serverUrl) return { apiBase: serverUrl, kind: "http" }

  return null
}

export function transportKey(transport: MyOpenPanelsTransport | null): string {
  if (!transport) return "none"
  return `http:${transport.apiBase}`
}

export function useMyOpenPanelsTransport(): MyOpenPanelsTransport | null {
  const [transport, setTransport] = useState(() => currentTransport())

  useEffect(() => {
    if (transport) return
    const syncTransport = () => {
      const nextTransport = currentTransport()
      if (nextTransport) {
        setTransport(nextTransport)
      }
    }
    const timer = window.setInterval(syncTransport, 100)
    window.addEventListener("message", syncTransport)
    window.addEventListener("openai:set_globals", syncTransport)
    syncTransport()
    return () => {
      window.clearInterval(timer)
      window.removeEventListener("message", syncTransport)
      window.removeEventListener("openai:set_globals", syncTransport)
    }
  }, [transport])

  return transport
}
