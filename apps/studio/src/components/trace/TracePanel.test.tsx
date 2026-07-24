import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { MyOpenPanelsI18nProvider } from "../../canvas"
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
    expect(automaticMarkup).toContain(
      "Waiting for an active Agent CLI to claim the task"
    )
    expect(automaticMarkup).not.toContain(
      "Please send the instruction to an Agent manually"
    )
    expect(manualMarkup).toContain(
      "Please send the instruction to an Agent manually"
    )
    expect(manualMarkup).toContain(">Copy instruction</button>")
    expect(manualMarkup).toContain("op-agent-task__copy-instruction")
    expect(manualMarkup).not.toContain(
      "Waiting for an active Agent CLI to claim the task"
    )
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
    expect(markup).toContain(">waiting for document conversion<")
    expect(markup).toContain("Generate Writing Document")
    expect(markup).not.toContain('aria-label="Task channel"')
    expect(markup).not.toContain('aria-label="Archive task"')
    expect(markup).toContain('aria-label="Delete task"')
    expect(markup).not.toContain("op-agent-task--warning")
    expect(markup).not.toContain(">Channel<")
    expect(markup).not.toContain("task:manual")
  })

  it("stacks pending Wiki updates behind one mutation instruction", () => {
    const markup = renderToStaticMarkup(
      <AgentPanel
        activeTab="tasks"
        buildInfo={{ channel: "release", label: "v0.4.17", version: "0.4.17" }}
        focusedTaskIds={null}
        hasUsableAgentCli={false}
        isOpen
        onClearFocusedTasks={() => undefined}
        onClose={() => undefined}
        onOpenManualTask={() => undefined}
        onOpenModelSettings={() => undefined}
        onTabChange={() => undefined}
        onTaskFilterChange={() => undefined}
        taskFilter="pending"
        tasks={[
          projectTask({
            capability: "wiki.ingestMarkdown",
            id: "task:wiki:first",
            mutationKey: "wiki:mutation",
            mutationSequence: 1,
            panelKind: "wiki",
            queue: "wiki",
            targetId: "wiki:default",
            type: "ingest_markdown_into_wiki",
          }),
          projectTask({
            blockedReason: "mutationPredecessor",
            capability: "wiki.ingestMarkdown",
            id: "task:wiki:second",
            mutationKey: "wiki:mutation",
            mutationSequence: 2,
            panelKind: "wiki",
            queue: "wiki",
            ready: false,
            targetId: "wiki:default",
            type: "maintain_wiki",
          }),
        ]}
        transport={{ apiBase: "http://127.0.0.1:43217", kind: "http" }}
      />
    )

    expect(markup.match(/op-agent-wiki-task-group"/g)).toHaveLength(1)
    expect(markup).toContain("2 subtasks")
    expect(markup).toContain("Import Markdown into Wiki")
    expect(markup).toContain("Update Wiki")
    expect(markup).not.toContain("wiki.ingestMarkdown")
    expect(markup).not.toContain("waiting for earlier update")
    expect(
      markup.match(/aria-label="Copy Wiki update instruction"/g)
    ).toHaveLength(1)
    expect(markup.match(/aria-label="Delete task"/g)).toHaveLength(2)
    expect(markup).not.toContain('aria-label="Copy task instruction"')
  })

  it("expands an individual Wiki update task to show its details", () => {
    const wikiTask = projectTask({
      dependencies: [
        {
          failurePolicy: "fail",
          prerequisiteTaskId: "task:conversion",
          status: "succeeded",
          successCondition: "succeeded",
        },
      ],
      id: "task:wiki:detail",
      mutationKey: "wiki:mutation",
      panelKind: "wiki",
      queue: "wiki",
      targetId: "wiki:default",
      type: "maintain_wiki",
    })
    const markup = renderToStaticMarkup(
      <AgentPanel
        activeTab="tasks"
        buildInfo={{ channel: "release", label: "v0.4.17", version: "0.4.17" }}
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

    expect(markup).toContain('aria-expanded="true"')
    expect(markup).toContain('aria-label="Copy task detail"')
    expect(markup).toContain("task:wiki:detail")
    expect(markup).toContain("task:conversion")
    expect(markup).toContain(">Prerequisites<")
  })

  it("localizes task status, name, filters, and readiness in Chinese", () => {
    const markup = renderToStaticMarkup(
      <MyOpenPanelsI18nProvider initialLocale="zh-CN">
        <AgentPanel
          activeTab="tasks"
          buildInfo={{
            channel: "release",
            label: "v0.4.17",
            version: "0.4.17",
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
          tasks={[
            projectTask({
              blockedReason: "prerequisite",
              ready: false,
              type: "generate_publication_cover",
            }),
          ]}
          transport={{ apiBase: "http://127.0.0.1:43217", kind: "http" }}
        />
      </MyOpenPanelsI18nProvider>
    )

    expect(markup).toContain("<strong>任务</strong>")
    expect(markup).toContain(">等待中<")
    expect(markup).toContain("生成出版封面")
    expect(markup).toContain("等待文档转换")
    expect(markup).toContain(">全部<")
  })

  it("shows document conversion separately from its Wiki update group", () => {
    const conversion = projectTask({
      capability: "wiki.convertDocument",
      id: "task:convert",
      panelKind: "wiki",
      queue: "wiki",
      targetId: "raw:document",
      type: "convert_document_to_markdown",
    })
    const markup = renderToStaticMarkup(
      <AgentPanel
        activeTab="tasks"
        buildInfo={{ channel: "release", label: "v0.4.17", version: "0.4.17" }}
        focusedTaskIds={null}
        hasUsableAgentCli={false}
        isOpen
        onClearFocusedTasks={() => undefined}
        onClose={() => undefined}
        onOpenManualTask={() => undefined}
        onOpenModelSettings={() => undefined}
        onTabChange={() => undefined}
        onTaskFilterChange={() => undefined}
        taskFilter="pending"
        tasks={[
          conversion,
          projectTask({
            blockedReason: "prerequisite",
            capability: "wiki.ingestMarkdown",
            dependencies: [
              {
                failurePolicy: "fail",
                prerequisiteTaskId: conversion.id,
                status: "queued",
                successCondition: "succeeded",
              },
            ],
            id: "task:wiki:ingest",
            mutationKey: "wiki:mutation",
            mutationSequence: 1,
            panelKind: "wiki",
            queue: "wiki",
            ready: false,
            targetId: "wiki:default",
            type: "ingest_markdown_into_wiki",
          }),
        ]}
        transport={{ apiBase: "http://127.0.0.1:43217", kind: "http" }}
      />
    )

    expect(markup.match(/op-agent-wiki-task-group"/g)).toHaveLength(1)
    expect(markup).toContain("Document conversion")
    expect(markup).not.toContain(">wiki.convertDocument</strong>")
    expect(
      markup.match(/aria-label="Copy Wiki update instruction"/g)
    ).toHaveLength(1)
    expect(markup.match(/aria-label="Copy task instruction"/g)).toHaveLength(1)
  })

  it("treats a Wiki mutation group as active while one child is running", () => {
    const markup = renderToStaticMarkup(
      <AgentPanel
        activeTab="tasks"
        buildInfo={{ channel: "release", label: "v0.4.17", version: "0.4.17" }}
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
        tasks={[
          projectTask({
            id: "task:wiki:running",
            mutationKey: "wiki:mutation",
            panelKind: "wiki",
            queue: "wiki",
            status: "running",
            targetId: "wiki:default",
            type: "maintain_wiki",
          }),
          projectTask({
            id: "task:wiki:queued",
            mutationKey: "wiki:mutation",
            panelKind: "wiki",
            queue: "wiki",
            targetId: "wiki:default",
            type: "maintain_wiki",
          }),
        ]}
        transport={{ apiBase: "http://127.0.0.1:43217", kind: "http" }}
      />
    )

    expect(markup).toContain("op-agent-wiki-task-group")
    expect(markup).toContain("Agent is processing Wiki update tasks")
    expect(markup.match(/op-agent-task__status-spinner/g)).toHaveLength(1)
    expect(markup).not.toContain("Copy Wiki update instruction")
    expect(markup).toContain("Active tasks (1)")
    expect(markup).toContain("Pending tasks (0)")
  })

  it("offers archive for one terminal focused Wiki task", () => {
    const wikiTask = projectTask({
      id: "task:wiki",
      mutationKey: "wiki:mutation",
      panelId: "panel:wiki",
      panelKind: "wiki",
      queue: "wiki",
      status: "succeeded",
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
        taskFilter="done"
        tasks={[wikiTask]}
        transport={{ apiBase: "http://127.0.0.1:43217", kind: "http" }}
      />
    )

    expect(markup).not.toContain("0/1 complete")
    expect(markup).toContain("op-agent-task-focus--single")
    expect(markup).toContain('aria-label="Back to all tasks"')
    expect(markup).not.toContain("Distillation tasks")
    expect(markup.match(/aria-label="Archive task"/g)).toHaveLength(1)
    expect(markup).not.toContain("Cancel task")
  })

  it("offers direct retry and an Agent message for an expanded failed task", () => {
    const failedTask = projectTask({
      error: { message: "failed" },
      status: "failed",
    })
    const markup = renderToStaticMarkup(
      <AgentPanel
        activeTab="tasks"
        buildInfo={{
          agentCli: "scripts/myopenpanels-dev",
          channel: "development",
          label: "dev",
          version: "0.4.16",
        }}
        focusedTaskIds={[failedTask.id]}
        hasUsableAgentCli
        isOpen
        onClearFocusedTasks={() => undefined}
        onClose={() => undefined}
        onOpenManualTask={() => undefined}
        onOpenModelSettings={() => undefined}
        onTabChange={() => undefined}
        onTaskFilterChange={() => undefined}
        taskFilter="done"
        tasks={[failedTask]}
        transport={{ apiBase: "http://127.0.0.1:43217", kind: "http" }}
      />
    )

    expect(markup).toContain('aria-label="Retry task"')
    expect(markup).toContain('aria-label="Copy Agent message"')
    expect(markup).toContain(">Retry task<")
    expect(markup).toContain(">Copy Agent message<")
  })

  it("does not offer archive or manual instructions for active tasks", () => {
    const runningTask = projectTask({
      dispatchState: "running",
      ready: false,
      status: "running",
    })
    const markup = renderToStaticMarkup(
      <AgentPanel
        activeTab="tasks"
        buildInfo={{ channel: "release", label: "v0.4.15", version: "0.4.15" }}
        focusedTaskIds={[runningTask.id]}
        hasUsableAgentCli
        isOpen
        onClearFocusedTasks={() => undefined}
        onClose={() => undefined}
        onOpenManualTask={() => undefined}
        onOpenModelSettings={() => undefined}
        onTabChange={() => undefined}
        onTaskFilterChange={() => undefined}
        taskFilter="active"
        tasks={[runningTask]}
        transport={{ apiBase: "http://127.0.0.1:43217", kind: "http" }}
      />
    )

    expect(markup).not.toContain('aria-label="Task channel"')
    expect(markup).not.toContain('aria-label="Archive task"')
    expect(markup).not.toContain('aria-label="Delete task"')
    expect(markup).not.toContain("Copy task instruction")
    expect(markup).not.toContain(">not ready<")
  })

  it("does not expose internal claim readiness as task metadata", () => {
    const readyTask = projectTask()
    const markup = renderToStaticMarkup(
      <AgentPanel
        activeTab="tasks"
        buildInfo={{ channel: "release", label: "v0.4.15", version: "0.4.15" }}
        focusedTaskIds={[readyTask.id]}
        hasUsableAgentCli
        isOpen
        onClearFocusedTasks={() => undefined}
        onClose={() => undefined}
        onOpenManualTask={() => undefined}
        onOpenModelSettings={() => undefined}
        onTabChange={() => undefined}
        onTaskFilterChange={() => undefined}
        taskFilter="pending"
        tasks={[readyTask]}
        transport={{ apiBase: "http://127.0.0.1:43217", kind: "http" }}
      />
    )

    expect(markup).not.toContain(">ready<")
    expect(markup).not.toContain(">not ready<")
  })

  it("offers exact Task Handoff without per-task runner routing", () => {
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
        tasks={[projectTask({ id: "task:queued", status: "queued" })]}
        transport={{ apiBase: "http://127.0.0.1:43217", kind: "http" }}
      />
    )

    expect(markup).toContain("Copy task instruction")
    expect(markup).not.toContain('aria-label="Task channel"')
    expect(markup).not.toContain(">Prefer<")
    expect(markup).not.toContain(">Automatic<")
  })
})
