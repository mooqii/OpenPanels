export const RUNTIME_RECONNECT_NOTICE_MS = 1500
export const RUNTIME_RECONNECT_TIMEOUT_MS = 30_000
export const RUNTIME_SAVE_GRACE_MS = 5000
export const RUNTIME_VISIBLE_POLL_MS = 2000
export const RUNTIME_HIDDEN_POLL_MS = 10_000
export const RUNTIME_RELOAD_MARKER = "myopenpanels:runtime-reload-version"

export type RuntimeVersionDecision = "current" | "reload" | "stale"
export type RuntimeConnectionDecision = "quiet" | "reconnecting" | "failed"

export function runtimeVersionDecision({
  attemptedVersion,
  loadedVersion,
  serverVersion,
}: {
  attemptedVersion: string | null
  loadedVersion: string
  serverVersion: string
}): RuntimeVersionDecision {
  if (serverVersion === loadedVersion) return "current"
  return attemptedVersion === serverVersion ? "stale" : "reload"
}

export function runtimePollDelay(hidden: boolean): number {
  return hidden ? RUNTIME_HIDDEN_POLL_MS : RUNTIME_VISIBLE_POLL_MS
}

export function runtimeConnectionDecision(
  disconnectedAt: number,
  now: number
): RuntimeConnectionDecision {
  const elapsed = now - disconnectedAt
  if (elapsed >= RUNTIME_RECONNECT_TIMEOUT_MS) return "failed"
  if (elapsed >= RUNTIME_RECONNECT_NOTICE_MS) return "reconnecting"
  return "quiet"
}

export async function flushBeforeRuntimeReload({
  flush,
  isDirty,
  now = Date.now,
  wait = (delay: number) =>
    new Promise<void>((resolve) => window.setTimeout(resolve, delay)),
}: {
  flush: () => Promise<void>
  isDirty: () => boolean
  now?: () => number
  wait?: (delay: number) => Promise<void>
}): Promise<void> {
  const deadline = now() + RUNTIME_SAVE_GRACE_MS
  while (isDirty() && now() < deadline) {
    try {
      await flush()
    } catch (error) {
      if (error instanceof Error && error.message === "HTTP 409") return
      await wait(350)
    }
  }
}
