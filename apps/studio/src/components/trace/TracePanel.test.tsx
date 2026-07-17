import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { AgentPanel } from "./TracePanel"

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
    const task = {
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
    }
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

    expect(manualMarkup).toContain("Copy task instruction")
    expect(automaticMarkup).not.toContain("Copy task instruction")
  })
})
