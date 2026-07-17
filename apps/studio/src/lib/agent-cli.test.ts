import { describe, expect, it } from "vitest"
import type { LocalCliInfo, ModelGatewaySettings } from "../types"
import { hasUsableAgentCli } from "./agent-cli"

const settings: ModelGatewaySettings = {
  byok: { baseUrl: null, model: null, providerId: null },
  localCli: {
    enabledProviderIds: ["codex"],
    executablePaths: {},
    model: null,
    providerId: "codex",
    providerModels: {},
    providerOrder: ["codex"],
    providerReasoning: {},
    reasoning: null,
  },
  maxConcurrency: 2,
  mode: "localCli",
}

function cli(overrides: Partial<LocalCliInfo> = {}): LocalCliInfo {
  return {
    authStatus: "ok",
    available: true,
    bin: "codex",
    id: "codex",
    models: [],
    modelsSource: "fallback",
    name: "Codex",
    reasoningOptions: [],
    ...overrides,
  }
}

describe("hasUsableAgentCli", () => {
  it("requires the CLI to be enabled and healthy", () => {
    expect(hasUsableAgentCli(settings, [cli()])).toBe(true)
    expect(
      hasUsableAgentCli(settings, [cli({ diagnostic: "version failed" })])
    ).toBe(false)
    expect(hasUsableAgentCli(settings, [cli({ authStatus: "missing" })])).toBe(
      false
    )
  })

  it("ignores healthy CLIs that are not enabled", () => {
    expect(hasUsableAgentCli(settings, [cli({ id: "claude" })])).toBe(false)
  })
})
