import { describe, expect, it } from "vitest"
import {
  flushBeforeRuntimeReload,
  RUNTIME_HIDDEN_POLL_MS,
  RUNTIME_RECONNECT_NOTICE_MS,
  RUNTIME_RECONNECT_TIMEOUT_MS,
  RUNTIME_VISIBLE_POLL_MS,
  runtimeConnectionDecision,
  runtimePollDelay,
  runtimeVersionDecision,
} from "./studio-runtime"

describe("studio runtime version decisions", () => {
  it("keeps the page when the service version is unchanged", () => {
    expect(
      runtimeVersionDecision({
        attemptedVersion: null,
        loadedVersion: "0.3.1",
        serverVersion: "0.3.1",
      })
    ).toBe("current")
  })

  it("reloads once when a new service version appears", () => {
    expect(
      runtimeVersionDecision({
        attemptedVersion: null,
        loadedVersion: "0.3.1",
        serverVersion: "0.3.2",
      })
    ).toBe("reload")
    expect(
      runtimeVersionDecision({
        attemptedVersion: "0.3.2",
        loadedVersion: "0.3.1",
        serverVersion: "0.3.2",
      })
    ).toBe("stale")
  })

  it("backs off while the page is hidden", () => {
    expect(runtimePollDelay(false)).toBe(RUNTIME_VISIBLE_POLL_MS)
    expect(runtimePollDelay(true)).toBe(RUNTIME_HIDDEN_POLL_MS)
  })

  it("moves a disconnected page from quiet recovery to failure", () => {
    expect(runtimeConnectionDecision(1000, 1000)).toBe("quiet")
    expect(
      runtimeConnectionDecision(1000, 1000 + RUNTIME_RECONNECT_NOTICE_MS)
    ).toBe("reconnecting")
    expect(
      runtimeConnectionDecision(1000, 1000 + RUNTIME_RECONNECT_TIMEOUT_MS)
    ).toBe("failed")
  })

  it("flushes pending canvas state before allowing a reload", async () => {
    let dirty = true
    let attempts = 0
    let clock = 0
    await flushBeforeRuntimeReload({
      flush: async () => {
        attempts += 1
        if (attempts === 1) throw new Error("offline")
        dirty = false
      },
      isDirty: () => dirty,
      now: () => clock,
      wait: async (delay) => {
        clock += delay
      },
    })

    expect(attempts).toBe(2)
    expect(dirty).toBe(false)
  })
})
