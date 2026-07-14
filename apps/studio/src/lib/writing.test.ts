import { describe, expect, it } from "vitest"
import { emptyWritingState } from "./api"
import {
  activeWritingSkillIds,
  toggleWritingSkillSelection,
  writingSkillSelectionError,
} from "./writing"

describe("Writing Skill selection", () => {
  it("selects the default Skill for a project without saved state", () => {
    expect(emptyWritingState().selectedCreateWritingSkillIds).toEqual([
      "writing-default",
    ])
    expect(emptyWritingState().selectedRevisionWritingSkillId).toBe(
      "writing-default"
    )
  })

  it("rejects an explicitly empty selection", () => {
    expect(writingSkillSelectionError("create", [])).toBe("required")
  })

  it("does not require an authoring Skill in refinement mode", () => {
    expect(activeWritingSkillIds("refine", ["writing-a"], "writing-b")).toEqual(
      []
    )
    expect(writingSkillSelectionError("refine", [])).toBeNull()
  })

  it("allows multiple Skills when creating a document", () => {
    expect(
      writingSkillSelectionError("create", ["writing-a", "writing-b"])
    ).toBeNull()
  })

  it("keeps create and revision selections independent", () => {
    const createIds = ["writing-a", "writing-b"]
    expect(activeWritingSkillIds("create", createIds, "writing-c")).toEqual(
      createIds
    )
    expect(activeWritingSkillIds("revise", createIds, "writing-c")).toEqual([
      "writing-c",
    ])
  })

  it("requires one Skill for revision and resolves a prior multi-selection", () => {
    expect(
      writingSkillSelectionError("revise", ["writing-a", "writing-b"])
    ).toBe("revision_limit")
    expect(
      toggleWritingSkillSelection(
        ["writing-a", "writing-b"],
        "writing-b",
        false,
        "revise"
      )
    ).toEqual(["writing-b"])
  })

  it("retains and toggles multiple create-mode selections", () => {
    const selected = toggleWritingSkillSelection(
      ["writing-a"],
      "writing-b",
      true,
      "create"
    )
    expect(selected).toEqual(["writing-a", "writing-b"])
    expect(
      toggleWritingSkillSelection(selected, "writing-a", false, "create")
    ).toEqual(["writing-b"])
  })
})
