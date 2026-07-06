import type {
  InsertArtifactInput,
  OpenPanelInput,
  OpenPanelsArtifact,
  OpenPanelsPanel,
  OpenPanelsSession,
} from "@openpanels/protocol"

const TRAILING_SLASHES_REGEX = /\/+$/

export interface OpenPanelsClientOptions {
  endpoint: string
  fetch?: typeof fetch
}

export function createOpenPanelsClient(options: OpenPanelsClientOptions) {
  const fetchImpl = options.fetch ?? fetch
  const endpoint = options.endpoint.replace(TRAILING_SLASHES_REGEX, "")

  async function post<T>(path: string, body: unknown): Promise<T> {
    const response = await fetchImpl(`${endpoint}${path}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    })
    if (!response.ok) {
      throw new Error(`OpenPanels request failed: ${response.status}`)
    }
    return response.json() as Promise<T>
  }

  async function get<T>(path: string): Promise<T> {
    const response = await fetchImpl(`${endpoint}${path}`)
    if (!response.ok) {
      throw new Error(`OpenPanels request failed: ${response.status}`)
    }
    return response.json() as Promise<T>
  }

  return {
    createSession(input: { title?: string } = {}) {
      return post<OpenPanelsSession>("/api/sessions", input)
    },
    listSessions() {
      return get<OpenPanelsSession[]>("/api/sessions")
    },
    openPanel(input: OpenPanelInput) {
      return post<OpenPanelsPanel>("/api/panels", input)
    },
    insertArtifact(input: InsertArtifactInput) {
      return post<OpenPanelsArtifact>("/api/artifacts", input)
    },
  }
}
