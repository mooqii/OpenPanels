import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import type { ProjectTask } from "../../types"
import { AgentPanel } from "./TracePanel"

function projectTask(overrides: Partial<ProjectTask> = {}): ProjectTask {
  return {
    createdAt: "2026-07-17T00:00:00Z",
    id: "task:manual",
    panelId: "panel:writing",
    panelKind: "writing",
    projectId: "project:test",
    queue: "writing",
    ready: true,
    status: "queued",
    targetId: "document:test",
    type: "generate_writing_document",
    updatedAt: "2026-07-17T00:00:00Z",
    ...overrides,
  }
}

describe("AgentPanel release UI", () => {
  it("renders Tasks as a static title without Communication", () => {
    const markup = renderToStaticMarkup(
      <AgentPanel
        activeTab="communication"
        buildInfo={{
          channel: "release",
          label: "v0.4.11",
          version: "0.4.11",
        }}
        focusedTaskIds={null}
        hasUsableAgentCli
        isOpen
        onClearFocusedTasks={() => undefined}
        onClose={() => undefined}
        onOpenManualTask={() => undefined}
        onOpenModelSettings={() => undefined}
        onTabChange={() => undefined}
        onTaskFilterChange={() => undefined}
        taskFilter="pending"
        tasks={[]}
        transport={{ apiBase: "http://127.0.0.1:43217", kind: "http" }}
      />
    )

    expect(markup).toContain("<strong>Tasks</strong>")
    expect(markup).not.toContain("Communication")
    expect(markup).not.toContain("Agent panel pages")
    expect(markup).not.toContain("pending task")
  })

  it("renders Communication without pause or connection status", () => {
    const markup = renderToStaticMarkup(
      <AgentPanel
        activeTab="communication"
        buildInfo={{
          channel: "development",
          label: "dev",
          version: "0.4.11",
        }}
        focusedTaskIds={null}
        hasUsableAgentCli
        isOpen
        onClearFocusedTasks={() => undefined}
        onClose={() => undefined}
        onOpenManualTask={() => undefined}
        onOpenModelSettings={() => undefined}
        onTabChange={() => undefined}
        onTaskFilterChange={() => undefined}
        taskFilter="pending"
        tasks={[]}
        transport={{ apiBase: "http://127.0.0.1:43217", kind: "http" }}
      />
    )

    expect(markup).toContain("Communication")
    expect(markup).toContain("Clear communication view")
    expect(markup).not.toContain("Pause communication stream")
    expect(markup).not.toContain(">live<")
    expect(markup).not.toContain(">paused<")
  })

  it("keeps task-processing controls after the scrollable task content", () => {
    const markup = renderToStaticMarkup(
      <AgentPanel
        activeTab="tasks"
        buildInfo={{
          channel: "release",
          label: "v0.4.11",
          version: "0.4.11",
        }}
        focusedTaskIds={null}
        hasUsableAgentCli
        isOpen
        onClearFocusedTasks={() => undefined}
        onClose={() => undefined}
        onOpenManualTask={() => undefined}
        onOpenModelSettings={() => undefined}
        onTabChange={() => undefined}
        onTaskFilterChange={() => undefined}
        taskFilter="pending"
        tasks={[]}
        transport={{ apiBase: "http://127.0.0.1:43217", kind: "http" }}
      />
    )

    expect(markup).toContain("Task processing")
    expect(markup).toContain("2 parallel")
    expect(markup.indexOf("No project tasks yet.")).toBeLessThan(
      markup.indexOf("Task processing")
    )
  })

  it("shows manual instructions beside pending task channels only without a usable CLI", () => {
    const task = projectTask()
    const commonProps = {
      activeTab: "tasks" as const,
      buildInfo: {
        channel: "release" as const,
        label: "v0.4.11",
        version: "0.4.11",
      },
      focusedTaskIds: null,
      isOpen: true,
      onClearFocusedTasks: () => undefined,
      onClose: () => undefined,
      onOpenManualTask: () => undefined,
      onOpenModelSettings: () => undefined,
      onTabChange: () => undefined,
      onTaskFilterChange: () => undefined,
      taskFilter: "pending" as const,
      tasks: [task],
      transport: {
        apiBase: "http://127.0.0.1:43217",
        kind: "http" as const,
      },
    }

    const manualMarkup = renderToStaticMarkup(
      <AgentPanel {...commonProps} hasUsableAgentCli={false} />
    )
    const automaticMarkup = renderToStaticMarkup(
      <AgentPanel {...commonProps} hasUsableAgentCli />
    )

    expect(manualMarkup).toContain("Send instruction manually")
    expect(manualMarkup).toContain("Copy instruction")
    expect(manualMarkup).toContain("Copy Project drain instruction")
    expect(manualMarkup).not.toContain("2 parallel")
    expect(manualMarkup.indexOf("Task processing")).toBeLessThan(
      manualMarkup.indexOf("Copy Project drain instruction")
    )
    expect(manualMarkup.indexOf("Copy Project drain instruction")).toBeLessThan(
      manualMarkup.indexOf("Model and Agent settings")
    )
    expect(automaticMarkup).toContain("Copy task instruction")
    expect(automaticMarkup).toContain("2 parallel")
    expect(automaticMarkup).not.toContain("Send instruction manually")
    expect(automaticMarkup).not.toContain("Copy Project drain instruction")
  })

  it("disables the Project drain instruction when no tasks are pending", () => {
    const markup = renderToStaticMarkup(
      <AgentPanel
        activeTab="tasks"
        buildInfo={{ channel: "release", label: "v0.4.15", version: "0.4.15" }}
        focusedTaskIds={null}
        hasUsableAgentCli={false}
        isOpen
        onClearFocusedTasks={() => undefined}
        onClose={() => undefined}
        onOpenManualTask={() => undefined}
        onOpenModelSettings={() => undefined}
        onTabChange={() => undefined}
        onTaskFilterChange={() => undefined}
        taskFilter="all"
        tasks={[projectTask({ status: "succeeded" })]}
        transport={{ apiBase: "http://127.0.0.1:43217", kind: "http" }}
      />
    )

    expect(markup).toMatch(
      /<button(?=[^>]*aria-label="Copy Project drain instruction")(?=[^>]*disabled="")[^>]*>/
    )
  })

  it("renders the compact status, title, channel, time, and eligible delete action", () => {
    const markup = renderToStaticMarkup(
      <AgentPanel
        activeTab="tasks"
        buildInfo={{ channel: "release", label: "v0.4.15", version: "0.4.15" }}
        focusedTaskIds={null}
        hasUsableAgentCli
        isOpen
        onClearFocusedTasks={() => undefined}
        onClose={() => undefined}
        onOpenManualTask={() => undefined}
        onOpenModelSettings={() => undefined}
        onTabChange={() => undefined}
        onTaskFilterChange={() => undefined}
        taskFilter="pending"
        tasks={[projectTask({ blockedReason: "prerequisite", ready: false })]}
        transport={{ apiBase: "http://127.0.0.1:43217", kind: "http" }}
      />
    )

    expect(markup).toContain("op-agent-task__status")
    expect(markup).toContain(">queued<")
    expect(markup).toContain(">prerequisite<")
    expect(markup).toContain("Generate Writing Document")
    expect(markup).toContain('aria-label="Task channel"')
    expect(markup).toContain('aria-label="Delete task"')
    expect(markup).not.toContain("op-agent-task--warning")
    expect(markup).not.toContain(">Channel<")
    expect(markup).not.toContain("task:manual")
  })

  it("keeps aggregate deletion on expanded Wiki members only", () => {
    const wikiTask = projectTask({
      id: "task:wiki",
      mutationKey: "wiki:mutation",
      panelId: "panel:wiki",
      panelKind: "wiki",
      queue: "wiki",
      targetId: "wiki:default",
      type: "maintain_wiki",
    })
    const markup = renderToStaticMarkup(
      <AgentPanel
        activeTab="tasks"
        buildInfo={{ channel: "release", label: "v0.4.15", version: "0.4.15" }}
        focusedTaskIds={[wikiTask.id]}
        hasUsableAgentCli
        isOpen
        onClearFocusedTasks={() => undefined}
        onClose={() => undefined}
        onOpenManualTask={() => undefined}
        onOpenModelSettings={() => undefined}
        onTabChange={() => undefined}
        onTaskFilterChange={() => undefined}
        taskFilter="pending"
        tasks={[wikiTask]}
        transport={{ apiBase: "http://127.0.0.1:43217", kind: "http" }}
      />
    )

    expect(markup).toContain("0/1 complete")
    expect(markup).toContain("op-agent-task-focus--single")
    expect(markup).toContain('aria-label="Back to all tasks"')
    expect(markup).not.toContain("Refinement tasks")
    expect(markup.match(/aria-label="Delete task"/g)).toHaveLength(1)
    expect(markup).not.toContain("Cancel task")
    expect(markup).not.toContain("Archive task")
  })

  it("does not offer deletion or manual instructions for active tasks", () => {
    const markup = renderToStaticMarkup(
      <AgentPanel
        activeTab="tasks"
        buildInfo={{ channel: "release", label: "v0.4.15", version: "0.4.15" }}
        focusedTaskIds={null}
        hasUsableAgentCli
        isOpen
        onClearFocusedTasks={() => undefined}
        onClose={() => undefined}
        onOpenManualTask={() => undefined}
        onOpenModelSettings={() => undefined}
        onTabChange={() => undefined}
        onTaskFilterChange={() => undefined}
        taskFilter="active"
        tasks={[projectTask({ status: "running" })]}
        transport={{ apiBase: "http://127.0.0.1:43217", kind: "http" }}
      />
    )

    expect(markup).toContain('aria-label="Task channel"')
    expect(markup).not.toContain('aria-label="Delete task"')
    expect(markup).not.toContain("Copy task instruction")
  })

  it("shows the recorded executor instead of automatic selection after dispatch", () => {
    const markup = renderToStaticMarkup(
      <AgentPanel
        activeTab="tasks"
        buildInfo={{ channel: "release", label: "v0.4.15", version: "0.4.15" }}
        focusedTaskIds={null}
        hasUsableAgentCli
        isOpen
        onClearFocusedTasks={() => undefined}
        onClose={() => undefined}
        onOpenManualTask={() => undefined}
        onOpenModelSettings={() => undefined}
        onTabChange={() => undefined}
        onTaskFilterChange={() => undefined}
        taskFilter="all"
        tasks={[
          projectTask({
            executionMethod: { kind: "localCli", providerId: "codex" },
            id: "task:codex",
            status: "running",
          }),
          projectTask({
            executionMethod: { kind: "localCli", providerId: "hermes" },
            id: "task:hermes",
            status: "succeeded",
          }),
          projectTask({
            executionMethod: { kind: "manualInstruction" },
            id: "task:manual-finished",
            status: "succeeded",
          }),
        ]}
        transport={{ apiBase: "http://127.0.0.1:43217", kind: "http" }}
      />
    )

    expect(markup).toContain("Codex CLI")
    expect(markup).toContain("Hermes")
    expect(markup).toContain("Manual instruction")
    expect(markup).not.toContain(">Automatic<")
  })
})
