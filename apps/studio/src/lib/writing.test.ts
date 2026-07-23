import { describe, expect, it } from "vitest"
import type { MyDocument, ProjectTask } from "../types"
import { emptyWritingState, normalizeWritingState } from "./api"
import {
  activeWritingSkillIds,
  distillationTaskGroups,
  latestWritingTaskForDocument,
  selectSingleSkill,
  sortMyDocumentsByActivity,
  toggleWritingSkillSelection,
  writingDocumentStatus,
  writingReferenceSelectionError,
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
    expect(emptyWritingState().createDraft).toBe("")
    expect(emptyWritingState().revisionDraft).toBe("")
  })

  it("normalizes legacy refinement state into distillation state", () => {
    expect(
      normalizeWritingState({
        createDraft: "Create this",
        draft: "",
        mode: "refine",
        refinementName: "House style",
        revisionDraft: "Revise this",
        selectedCreateWritingSkillIds: ["writing-default"],
        selectedRefinementSkillId: "writing-refinement-default",
        selectedRevisionWritingSkillId: "writing-default",
        targetMyDocumentId: null,
      })
    ).toMatchObject({
      distillationName: "House style",
      mode: "distill",
      selectedDistillationSkillId: "writing-distillation-default",
    })
  })

  it("rejects an explicitly empty selection", () => {
    expect(writingSkillSelectionError("create", [])).toBe("required")
  })

  it("does not require an authoring Skill in distillation mode", () => {
    expect(
      activeWritingSkillIds("distill", ["writing-a"], "writing-b")
    ).toEqual([])
    expect(writingSkillSelectionError("distill", [])).toBeNull()
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

  it("switches single selection without clearing the selected Skill", () => {
    expect(selectSingleSkill("writing-a", "writing-b", true)).toBe("writing-b")
    expect(selectSingleSkill("writing-b", "writing-b", false)).toBe("writing-b")
  })

  it("requires ready references only when creating a document", () => {
    expect(writingReferenceSelectionError("create", 0, 0)).toBe("required")
    expect(writingReferenceSelectionError("create", 1, 1)).toBe("unready")
    expect(writingReferenceSelectionError("create", 1, 0)).toBeNull()
    expect(writingReferenceSelectionError("revise", 0, 0)).toBeNull()
  })
})

function task(
  id: string,
  status: string,
  updatedAt: string,
  options: { mode?: "create" | "revise"; targetId?: string; type?: string } = {}
): ProjectTask {
  return {
    createdAt: updatedAt,
    id,
    input: {
      mode: options.mode ?? "create",
      targetMyDocumentId: options.targetId ?? null,
    },
    panelId: "panel-writing",
    panelKind: "writing",
    projectId: "project-1",
    queue: "writing",
    status,
    targetId: options.targetId ?? id,
    type: options.type ?? "write_my_document",
    updatedAt,
  }
}

function document(id: string, updatedAt: string): MyDocument {
  return {
    contentRef: "content.md",
    contentVersion: 0,
    createdAt: updatedAt,
    format: "markdown",
    id,
    mimeType: "text/markdown",
    originalFileName: "untitled.md",
    publishHistory: [],
    taskId: null,
    threadId: null,
    title: "",
    updatedAt,
  }
}

describe("Writing task presentation", () => {
  it("groups only actionable distillation tasks", () => {
    const groups = distillationTaskGroups([
      task("waiting", "queued", "2026-01-01T00:00:00Z", {
        type: "distill_writing_skill",
      }),
      task("active", "running", "2026-01-02T00:00:00Z", {
        type: "distill_writing_skill",
      }),
      task("error", "failed", "2026-01-03T00:00:00Z", {
        type: "distill_writing_skill",
      }),
      task("done", "succeeded", "2026-01-04T00:00:00Z", {
        type: "distill_writing_skill",
      }),
    ])
    expect(groups.waiting.map(({ id }) => id)).toEqual(["waiting"])
    expect(groups.active.map(({ id }) => id)).toEqual(["active"])
    expect(groups.error.map(({ id }) => id)).toEqual(["error"])
  })

  it("maps document task states and hides terminal states", () => {
    expect(
      writingDocumentStatus(task("create", "queued", "2026-01-01T00:00:00Z"))
    ).toBe("pending_create")
    expect(
      writingDocumentStatus(
        task("revise", "queued", "2026-01-01T00:00:00Z", {
          mode: "revise",
        })
      )
    ).toBe("pending_revise")
    expect(
      writingDocumentStatus(task("active", "running", "2026-01-01T00:00:00Z"))
    ).toBe("active")
    expect(
      writingDocumentStatus(task("failed", "failed", "2026-01-01T00:00:00Z"))
    ).toBe("failed")
    expect(
      writingDocumentStatus(task("done", "succeeded", "2026-01-01T00:00:00Z"))
    ).toBeNull()
  })

  it("uses the latest linked task and sorts by effective activity", () => {
    const documents = [
      document("old", "2026-01-01T00:00:00Z"),
      document("new", "2026-01-03T00:00:00Z"),
    ]
    const tasks = [
      task("old-task", "queued", "2026-01-04T00:00:00Z", {
        targetId: "old",
      }),
      task("older-task", "failed", "2026-01-02T00:00:00Z", {
        targetId: "old",
      }),
    ]
    expect(latestWritingTaskForDocument(tasks, documents[0])?.id).toBe(
      "old-task"
    )
    const taskLinkedDocument = document("placeholder", "2026-01-01T00:00:00Z")
    taskLinkedDocument.taskId = "placeholder-task"
    expect(
      latestWritingTaskForDocument(
        [task("placeholder-task", "queued", "2026-01-05T00:00:00Z")],
        taskLinkedDocument
      )?.id
    ).toBe("placeholder-task")
    expect(
      sortMyDocumentsByActivity(documents, tasks).map(({ id }) => id)
    ).toEqual(["old", "new"])
  })
})
