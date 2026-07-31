import { describe, expect, it } from "vitest"
import {
  agentCliBoundaryInstruction,
  agentCliExecutable,
  agentRecoveryInstruction,
} from "./agent-instructions"

describe("Agent instructions", () => {
  it("keeps development instructions on the checkout-local CLI", () => {
    const runtime = {
      agentCli: "/checkout/scripts/myopenpanels-dev",
      channel: "development" as const,
    }
    expect(agentCliExecutable(runtime)).toBe(
      "/checkout/scripts/myopenpanels-dev"
    )
    expect(agentCliBoundaryInstruction(runtime, "zh-CN")).toContain(
      "不要运行已安装的正式版 myopenpanels"
    )
    expect(agentRecoveryInstruction(runtime)).toContain(
      "/checkout/scripts/myopenpanels-dev studio start"
    )
    expect(agentRecoveryInstruction(runtime)).toContain(
      "不要运行 myopenpanels update install"
    )
  })

  it("keeps release instructions on the installed CLI", () => {
    const runtime = { channel: "release" as const }
    expect(agentCliExecutable(runtime)).toBe("myopenpanels")
    expect(agentCliBoundaryInstruction(runtime, "en")).toBe(
      "Use the installed myopenpanels."
    )
    expect(agentCliBoundaryInstruction(runtime, "zh-CN")).toBe(
      "使用已安装的 myopenpanels。"
    )
    expect(agentRecoveryInstruction(runtime)).toContain(
      "myopenpanels update install"
    )
    expect(agentRecoveryInstruction(runtime)).not.toContain(
      "scripts/myopenpanels-dev"
    )
    expect(agentRecoveryInstruction(runtime)).not.toContain("开发版")
  })

  it("quotes an exact CLI path with spaces", () => {
    expect(
      agentCliExecutable({
        agentCli: "/checkout/My OpenPanels/scripts/myopenpanels-dev",
        channel: "development",
      })
    ).toBe("'/checkout/My OpenPanels/scripts/myopenpanels-dev'")
  })
})
