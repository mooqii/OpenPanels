import { describe, expect, it } from "vitest"
import type { ModelGatewaySettings } from "../../types"
import {
  withLocalCliProviderEnabled,
  withLocalCliProviderModel,
  withLocalCliProviderMoved,
  withLocalCliProviderOrder,
  withLocalCliProviderReasoning,
} from "./ModelGatewaySettings"

const settings = (): ModelGatewaySettings => ({
  byok: { baseUrl: null, model: null, providerId: null },
  localCli: {
    enabledProviderIds: ["codex", "hermes"],
    executablePaths: {},
    model: "gpt-5.4",
    providerModels: { codex: "gpt-5.4", hermes: "default" },
    providerId: "codex",
    providerOrder: ["codex", "hermes"],
    providerReasoning: { codex: "high", hermes: "default" },
    reasoning: "high",
  },
  maxConcurrency: 2,
  mode: "localCli",
})

describe("ModelGatewaySettingsDialog", () => {
  it("does not enable an unavailable CLI", () => {
    const current = settings()

    expect(withLocalCliProviderEnabled(current, "gemini", true, false)).toBe(
      current
    )
  })

  it("enables available CLIs without changing the primary provider", () => {
    const next = withLocalCliProviderEnabled(settings(), "opencode", true, true)

    expect(next.localCli.providerOrder).toEqual(["codex", "hermes", "opencode"])
    expect(next.localCli.providerId).toBe("codex")
    expect(next.localCli.model).toBe("gpt-5.4")
  })

  it("updates primary model defaults only when priority changes", () => {
    const next = withLocalCliProviderOrder(settings(), ["hermes", "codex"])

    expect(next.localCli.providerId).toBe("hermes")
    expect(next.localCli.model).toBe("default")
    expect(next.localCli.reasoning).toBe("default")
  })

  it("moves an enabled CLI to the dropped position", () => {
    const current = withLocalCliProviderEnabled(
      settings(),
      "opencode",
      true,
      true
    )
    const next = withLocalCliProviderMoved(current, "opencode", "codex")

    expect(next.localCli.providerOrder).toEqual(["opencode", "codex", "hermes"])
    expect(next.localCli.providerId).toBe("opencode")
  })

  it("configures a non-primary CLI without changing the primary CLI", () => {
    const modeled = withLocalCliProviderModel(settings(), "claude", "sonnet")
    const next = withLocalCliProviderReasoning(modeled, "claude", "medium")

    expect(next.localCli.providerId).toBe("codex")
    expect(next.localCli.model).toBe("gpt-5.4")
    expect(next.localCli.reasoning).toBe("high")
    expect(next.localCli.providerModels.claude).toBe("sonnet")
    expect(next.localCli.providerReasoning.claude).toBe("medium")
  })
})
