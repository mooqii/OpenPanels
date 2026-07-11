# CLI Agent Setup

MyOpenPanels can run in any local agent that can execute shell commands. Agents
use the `myopenpanels` CLI to start the MyOpenPanels Studio, inspect the active
project and panel, read panel state, and use panel-specific commands such as
canvas selection and image insertion.

## Install

Install the Rust-native `myopenpanels` CLI from GitHub Releases, then verify
it:

```bash
myopenpanels --version
```

The recommended agent skill uses the installed `myopenpanels` binary as the
stable entry point. Panel-specific instructions are returned by the CLI through
`agent bootstrap`, so users do not need to keep separate canvas/wiki skills
manually updated.

The compact context renderer, capability manifest, built-in workflow guides,
and built-in agent skills now live in the Rust CLI crate under
`crates/myopenpanels/src/agent.rs`, with markdown resources in
`agent-resources/`.

If you do not pass `--project-dir`, MyOpenPanels uses `MYOPENPANELS_PROJECT_DIR` or the
current working directory for project metadata. Canvas data is stored in the
global MyOpenPanels data directory so agents and projects can share the same
boards and assets.

The current project and studio process are isolated per agent conversation when
the agent exposes a thread/session environment variable such as
`CODEX_THREAD_ID` or a Hermes conversation id. A new conversation reuses the
most recently updated Project, and creates a default Project only when storage
contains no Projects.

## Agent Workflow

1. Run `myopenpanels studio start --local-only --project-dir <project> --format
   json`.
2. Treat CLI success as Studio readiness, not proof that the panel is visible.
   Follow `nextRequiredAction`: open its URL unchanged with a callable in-app
   URL opener, or immediately run `myopenpanels studio open-system-browser
   --local-only --project-dir <project> --format json` when that capability is
   absent, fails, or cannot report success. Stop an open-only task only after an
   opener succeeds.
3. Run `myopenpanels agent bootstrap --format json` only before panel-specific
   work. The returned context lists the current project, active
   panel, available panels, current state, and full command capabilities. The
   startup Bootstrap response may also report a newer MyOpenPanels Skill
   version and its canonical source; the Agent may decide whether to update it
   through the host environment's Skill installer.
4. Run `myopenpanels panel list --project-dir <project> --format json` or
   `myopenpanels panel switch --project-dir <project> --kind wiki --format json`
   to inspect or switch panels.
5. Before a new Wiki or Canvas operation, load the `activePanelSkill` returned
   by Bootstrap. Wiki authoring tasks load `wiki-panel` first, then the selected
   `karpathy-llm-wiki` or `karpathy-llm-wiki-zh` skill with the task id. Wiki
   queries and generated documents route through references in `wiki-panel`.
6. For Canvas work, load `canvas-panel`, then run `myopenpanels canvas selection read --project-dir
   <project> --format json` to inspect the current canvas selection.
7. Run `myopenpanels canvas selection read --project-dir <project>
   --include-image-base64 --format json` or `myopenpanels canvas selection
   export --project-dir <project> --output <path> --format json` when selected pixels
   are needed.
8. Run `myopenpanels canvas image insert --project-dir <project> --image <path>
   --placement right --format json` to place a generated local image into the
   canvas.

## Command Map

- `myopenpanels studio start`: start or reuse the MyOpenPanels Studio.
- `myopenpanels studio status`: show the conversation-scoped MyOpenPanels Studio process status.
- `myopenpanels studio open-system-browser`: explicitly open the studio URL in the system browser.
- `myopenpanels studio wait`: wait for the studio HTTP server to become ready.
- `myopenpanels studio stop`: stop the conversation-scoped MyOpenPanels Studio process.
- `myopenpanels agent bootstrap`: print the protocol v2 focus, state,
  capabilities, applicable guides, and active operations.
- `myopenpanels agent skills`: list loadable built-in skills.
- `myopenpanels agent skill <id>`: resolve a panel or authoring skill and
  print its task-specific loader context.
- `myopenpanels agent skill wiki-panel`: load Wiki knowledge, generated
  document, and authoring-skill routing rules.
- `myopenpanels agent skill canvas-panel`: load Canvas selection,
  generation, placement, and workflow-skill routing rules.
- `myopenpanels wiki selection read`: read whether the whole Wiki is
  selected and which raw documents the user selected directly.
- `myopenpanels wiki pages search`: search the selected Wiki space before
  reading relevant pages.
- `myopenpanels agent guides`: list loadable built-in guides.
- `myopenpanels agent guide <id>`: print one full workflow guide.
- `myopenpanels panel list`: list panels in the current Project.
- `myopenpanels panel current`: read the active Project panel.
- `myopenpanels panel switch`: switch the active Project panel.
- `myopenpanels wiki context`: read the current Wiki context.
- `myopenpanels canvas state`: read the current canvas session, panel, and state.
- `myopenpanels canvas selection read`: read selected shapes and optional PNG data.
- `myopenpanels canvas selection export`: write the exported selection PNG to a file.
- `myopenpanels canvas image insert`: add a local image file as a canvas image shape.

## WorkBuddy Troubleshooting

WorkBuddy's Results Panel is a UI surface, not by itself a callable URL-open
capability. Use an exposed URL-open or Preview tool when one is present; no
separate Agent Browser Skill is required. Otherwise use the system-browser
fallback returned by `nextRequiredAction`.

Troubleshoot the stages independently:

- No successful `studio start` payload, or a bind/timeout error: allow localhost
  binding in the WorkBuddy sandbox or temporarily use the required permission
  mode, then retry the local-only start command.
- A payload with `ok: true` and a URL, but no visible panel: the Studio is ready
  and the host open step is missing or failed; use the fallback command.
- `browser_open_failed`: the system launcher rejected the open request. Use the
  recovery URL manually and fix the host's external-program permission.

MyOpenPanels does not guess undocumented WorkBuddy session environment
variables. WorkBuddy conversations therefore use the CLI's normal default
context behavior until a stable host session identifier is available.

## Task Targets

MyOpenPanels only assigns background work to explicitly registered targets. A
target declares the capabilities it can execute, such as
`wiki.convertDocument` or `wiki.ingestMarkdown`.

Register a polling target and claim work atomically:

```bash
myopenpanels agent targets register \
  --name my-agent --transport poll \
  --capability wiki.ingestMarkdown --format json
myopenpanels tasks claim-next \
  --target-id <target-id> --wait-ms 25000 --format json
```

The claim response contains a lease token. Use it with `tasks heartbeat`,
`tasks complete`, `tasks fail`, or `tasks release`.

For a local command-based agent, the bridge owns this lifecycle automatically:

```bash
myopenpanels agent bridge \
  --name my-worker \
  --capability wiki.ingestMarkdown \
  --command '<agent command>'
```

Webhook targets register an endpoint and receive signed wake notifications.
The target must still claim the task before executing it. Use
`myopenpanels agent bridge status --format json` to inspect dispatcher,
target, retry, and running-task status.
