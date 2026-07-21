import { describe, expect, it } from "vitest"
import type {
  ProjectTask,
  PublishingAttempt,
  PublishingRelease,
} from "../types"
import {
  publishingAttemptStatus,
  publishingPublicationSummary,
  publishingSourceHasContent,
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

function release(attempts: PublishingAttempt[]): PublishingRelease {
  return {
    attempts,
    createdAt: "2026-07-20T00:00:00Z",
    id: "release:1",
    platform: "xiaohongshu",
    snapshot: { bodyText: "Body", media: [], title: "Title" },
    sourcePublicationId: "publication:1",
    sourceUpdatedAt: "2026-07-20T00:00:00Z",
    updatedAt: "2026-07-20T00:00:00Z",
  }
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
  it("allows either body text or cover images to be published", () => {
    expect(publishingSourceHasContent("Body", 0)).toBe(true)
    expect(publishingSourceHasContent("", 1)).toBe(true)
    expect(publishingSourceHasContent("  ", 0)).toBe(false)
  })

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

  it("does not treat an attempt whose task is no longer listed as queued", () => {
    expect(publishingAttemptStatus(attempt)).toBe("unknown")
  })

  it("summarizes successful publishes and only the latest unfinished status per skill", () => {
    const published = {
      ...attempt,
      completedAt: "2026-07-20T00:01:00Z",
      id: "attempt:published",
      outcome: "published" as const,
      phase: "completed" as const,
      updatedAt: "2026-07-20T00:01:00Z",
    }
    const retry = {
      ...attempt,
      createdAt: "2026-07-20T00:02:00Z",
      id: "attempt:retry",
      taskId: "task:retry",
      updatedAt: "2026-07-20T00:02:00Z",
    }
    const failed = {
      ...attempt,
      createdAt: "2026-07-20T00:03:00Z",
      id: "attempt:failed",
      skillId: "publishing-wechat",
      taskId: "task:failed",
      updatedAt: "2026-07-20T00:03:00Z",
    }

    expect(
      publishingPublicationSummary(
        [release([published, retry, failed])],
        [
          { ...task("reserved"), id: "task:retry" },
          { ...task("failed"), id: "task:failed" },
        ]
      )
    ).toEqual({ publishedCount: 1, statuses: ["error", "publishing"] })
  })
})
