import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { emptyWritingState } from "../../lib/api"
import type { ProjectTask, TaskStatus } from "../../types"
import { WritingComposer } from "./WritingComposer"

function distillationTask(id: string, status: TaskStatus): ProjectTask {
  const timestamp = "2026-07-28T00:00:00Z"
  return {
    createdAt: timestamp,
    id,
    panelId: "panel-writing",
    panelKind: "writing",
    projectId: "project-1",
    queue: "writing",
    status,
    targetId: id,
    type: "distill_writing_skill",
    updatedAt: timestamp,
  }
}

describe("WritingComposer", () => {
  it("shows a spinning indicator before an active distillation", () => {
    const markup = renderToStaticMarkup(
      <WritingComposer
        documents={[]}
        isSelectionBusy={false}
        onManageSkills={() => undefined}
        onOpenAgentTasks={() => undefined}
        onOpenLibrary={() => undefined}
        onReload={() => Promise.resolve()}
        selection={{ isWikiSelected: false, selectedMyDocumentIds: [] }}
        skillsRevision={0}
        state={emptyWritingState()}
        tasks={[
          distillationTask("active", "running"),
          distillationTask("waiting", "queued"),
        ]}
        transport={{ apiBase: "http://127.0.0.1:43217", kind: "http" }}
      />
    )

    expect(markup).toMatch(
      /op-writing-distillation-status--active.*op-wiki-spin.*1 distillation in progress/
    )
    expect(markup).not.toMatch(
      /op-writing-distillation-status--waiting.*op-wiki-spin/
    )
  })
})
