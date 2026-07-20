import { describe, expect, it } from "vitest"
import type { ProjectTask, PublishingAttempt } from "../types"
import {
  publishingAttemptStatus,
  typesettingContentToPlainText,
} from "./publishing"

const attempt: PublishingAttempt = {
  completedAt: null,
  createdAt: "2026-07-20T00:00:00Z",
  id: "attempt:1",
  mode: "auto",
  outcome: null,
  phase: "queued",
  reasonCode: null,
  remoteUrl: null,
  requestId: "request:1",
  skillHash: "sha256:test",
  skillId: "publishing-xiaohongshu",
  skillName: "Xiaohongshu",
  summary: null,
  taskId: "task:1",
}

function task(status: string): ProjectTask {
  return {
    createdAt: "2026-07-20T00:00:00Z",
    id: "task:1",
    panelId: "panel:publishing",
    panelKind: "publishing",
    projectId: "project:1",
    queue: "publishing",
    status,
    targetId: "release:1",
    type: "publish_xiaohongshu_note",
    updatedAt: "2026-07-20T00:00:00Z",
  }
}

describe("publishing helpers", () => {
  it("preserves visible structure and excludes inline images", () => {
    expect(
      typesettingContentToPlainText({
        type: "doc",
        content: [
          {
            type: "paragraph",
            content: [
              { type: "text", text: "First" },
              { type: "image", attrs: { src: "/inline.png" } },
              { type: "hardBreak" },
              { type: "text", text: "Second" },
            ],
          },
          {
            type: "orderedList",
            content: [
              {
                type: "listItem",
                content: [
                  {
                    type: "paragraph",
                    content: [{ type: "text", text: "One" }],
                  },
                ],
              },
              {
                type: "listItem",
                content: [
                  {
                    type: "paragraph",
                    content: [{ type: "text", text: "Two" }],
                  },
                ],
              },
            ],
          },
        ],
      })
    ).toBe("First\nSecond\n\n1. One\n\n2. Two")
  })

  it("maps committing interruption to unknown without retrying", () => {
    expect(
      publishingAttemptStatus(
        { ...attempt, phase: "committing" },
        task("failed")
      )
    ).toBe("unknown")
    expect(
      publishingAttemptStatus(
        { ...attempt, phase: "committing" },
        task("running")
      )
    ).toBe("committing")
  })

  it("uses the execution outcome as the terminal business status", () => {
    expect(
      publishingAttemptStatus(
        { ...attempt, outcome: "needs_user_action", phase: "completed" },
        task("succeeded")
      )
    ).toBe("needs_user_action")
  })
})
