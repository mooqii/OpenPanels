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

The bounded context renderer, Command Registry, built-in workflow guides,
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
   Follow `data.nextRequiredAction`: open its URL unchanged with a callable in-app
   URL opener, or execute `data.nextRequiredAction.fallback.argv` with the same
   resolved CLI executable when that capability is absent, fails, or cannot
   report success. Stop an open-only task only after an opener succeeds.
3. Run `myopenpanels agent bootstrap --project-dir <project> --format json`
   only before panel-specific
   work. Read the compact Protocol v4 payload from `data`; the complete envelope
   is capped at 8192 UTF-8 bytes. It identifies the current focus, bounded Panel
   context, work counts, and discovery references rather than embedding full
   commands or documents.
4. Follow `data.nextRequiredAction`. A rare `update-entry-skill` response must
   update or verify the Agent-host Entry Skill, execute its acknowledgement, and
   rerun Bootstrap before panel work. Otherwise, sequentially complete every
   `data.nextActions` entry referenced by `actionRefs`: execute `cli` actions
   with their `argv`, follow `agent-host` instructions without expecting an
   `argv`, and read every returned Skill file before choosing remaining actions.
5. Repeat the same response-driven loop. Never infer a business command from
   remembered paths, flags, or display command strings.

## Command Map

- `myopenpanels studio start`: start or reuse the MyOpenPanels Studio.
- `myopenpanels studio status`: show the conversation-scoped MyOpenPanels Studio process status.
- `myopenpanels studio open-system-browser`: explicitly open the studio URL in the system browser.
- `myopenpanels studio wait`: wait for the studio HTTP server to become ready.
- `myopenpanels studio stop`: stop the conversation-scoped MyOpenPanels Studio process.
- `myopenpanels agent bootstrap`: print the compact Protocol v4 focus, bounded
  context, work summaries, mandatory Skill action references, and
  progressive-discovery references. Load every referenced Skill before
  evaluating any other action; this includes the active Panel Skill and, when a
  ready Wiki Task exists, its captured authoring Skill.
- `myopenpanels agent entry-skill acknowledge`: confirm that the current Agent
  context has installed or verified the one-time required Entry Skill version.
- `myopenpanels agent capability list`: list scopes, or compact intents with
  `--scope <scope>`.
- `myopenpanels agent capability read --intent <intent>`: read one complete
  Command Registry descriptor.
- `myopenpanels agent skill list`: list compact Skill summaries, optionally
  filtered by `--panel-kind` and `--task-type`.
- `myopenpanels agent skill read --skill-id <id>`: resolve a panel or authoring
  skill, print its task-specific loader context, and return the required local
  `SKILL.md` read action.
- `myopenpanels agent skill read --skill-id wiki-panel`: load Wiki knowledge, generated
  document, and authoring-skill routing rules.
- `myopenpanels agent skill read --skill-id canvas-panel`: load Canvas selection,
  generation, placement, and workflow-skill routing rules.
- `myopenpanels panel selection read`: read the active panel's explicit selection
  through its Panel Module.
- `myopenpanels wiki page search`: search the selected Wiki space before
  reading relevant pages.
- `myopenpanels agent skill read --skill-id task-queue`: load the generic Task
  queue lifecycle contract when the request handles queued work.
- `myopenpanels panel list`: list panels in the current Project.
- `myopenpanels panel current`: read the active Project panel.
- `myopenpanels panel activate`: activate a Project panel; this is the only panel command that changes focus.
- `myopenpanels panel context read`: read compact context from the active Panel Module.
- `myopenpanels panel state read`: read the potentially large raw active-panel state.
- `myopenpanels canvas selection export`: write the explicit Canvas selection PNG to a file.
- `myopenpanels canvas image insert`: add a local image file as a Canvas image shape.
- `myopenpanels task ...`: operate the sole public Task lifecycle.
- `myopenpanels operation ...`: inspect and finish persistent Canvas or Wiki Operations.

Wiki selection details report whether the whole Wiki is
  selected and which raw documents the user selected directly.

## WorkBuddy Troubleshooting

WorkBuddy's Results Panel is a UI surface, not by itself a callable URL-open
capability. Use an exposed URL-open or Preview tool when one is present; no
separate Agent Browser Skill is required. Otherwise use the system-browser
fallback returned by `data.nextRequiredAction`.

Troubleshoot the stages independently:

- No successful `studio start` payload, or a bind/timeout error: allow localhost
  binding in the WorkBuddy sandbox or temporarily use the required permission
  mode, then retry the local-only start command.
- A payload with `ok: true` and `data.nextRequiredAction.url`, but no visible panel: the Studio is ready
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
myopenpanels agent target register \
  --name my-agent --transport poll \
  --capability wiki.ingestMarkdown --format json
myopenpanels task claim-next \
  --target-id <target-id> --wait-ms 25000 --format json
```

The claim response contains a lease token under `data.leaseToken`. Use it with
`task heartbeat`, `task complete`, `task fail`, or `task release`.

For a local command-based agent, the bridge owns this lifecycle automatically:

```bash
myopenpanels agent bridge run \
  --name my-worker \
  --capability wiki.ingestMarkdown \
  --command '<agent command>'
```

Webhook targets register an endpoint and receive signed wake notifications.
The target must still claim the task before executing it. Use
`myopenpanels agent bridge status --format json` to inspect dispatcher,
target, retry, and running-task status.
