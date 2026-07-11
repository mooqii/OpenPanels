# CLI Agent Setup

MyOpenPanels can run in any local agent that can execute shell commands. Agents
use the `openpanels-local` CLI to start the local studio, inspect the active
project and panel, read panel state, and use panel-specific commands such as
canvas selection and image insertion.

## Install

Install the Rust-native `openpanels-local` CLI from GitHub Releases, then verify
it:

```bash
openpanels-local --version
```

The recommended agent skill uses the installed `openpanels-local` binary as the
stable entry point. Panel-specific instructions are returned by the CLI through
`agent bootstrap`, so users do not need to keep separate canvas/wiki skills
manually updated.

The compact context renderer, capability manifest, built-in workflow guides,
and built-in agent skills now live in the Rust CLI crate under
`crates/openpanels-local/src/agent.rs`, with markdown resources in
`agent-resources/`.

If you do not pass `--project`, OpenPanels uses `OPENPANELS_PROJECT_DIR` or the
current working directory for project metadata. Canvas data is stored in the
global MyOpenPanels data directory so agents and projects can share the same
boards and assets.

The current project and studio process are isolated per agent conversation when
the agent exposes a thread/session environment variable such as
`CODEX_THREAD_ID` or a Hermes conversation id. A new conversation creates a new
MyOpenPanels Project on first use, while still allowing the user to switch to
any existing Project in the studio.

## Agent Workflow

1. Run `openpanels-local studio start --project <project> --format json`.
2. Open the returned `browserUrl` in the agent's in-app Browser side panel.
   `serverUrl` is kept as the localhost URL for same-computer use; `browserUrl`
   may use a LAN address when another device is viewing the agent.
3. Run `openpanels-local agent bootstrap --project <project> --format json` before
   panel-specific work. The returned context lists the current project, active
   panel, available panels, current state, and full command capabilities. On
   this startup Bootstrap only, compare the loaded MyOpenPanels Skill version
   with `entrySkill.requiredVersion`; a missing or older version is updated from
   `entrySkill.source` through the Agent host's Skill installer.
4. Run `openpanels-local panel list --project <project> --format json` or
   `openpanels-local panel switch --project <project> --kind wiki --format json`
   to inspect or switch panels.
5. For complex workflows, run `openpanels-local agent skills --project
   <project>` or `openpanels-local agent guides --project <project>`, then load
   the recommended skill or guide. Wiki tasks use
   `openpanels-local agent skill karpathy-llm-wiki --project <project> --task-id
   <task-id>`.
   For Wiki knowledge context, follow the CLI-suggested
   `wiki.knowledge-context` guide. It decides when the agent should load
   `wiki-query` without a task id.
6. For canvas work, run `openpanels-local canvas selection read --project
   <project> --format json` to inspect the current canvas selection.
7. Run `openpanels-local canvas selection read --project <project>
   --include-image-base64 --format json` or `openpanels-local canvas selection
   export --project <project> --output <path> --format json` when selected pixels
   are needed.
8. Run `openpanels-local canvas image insert --project <project> --image <path>
   --placement right --format json` to place a generated local image into the
   canvas.

## Command Map

- `openpanels-local studio start`: start or reuse the local studio.
- `openpanels-local studio status`: show the conversation-local studio process status.
- `openpanels-local studio open`: open the studio URL in the system browser.
- `openpanels-local studio wait`: wait for the studio HTTP server to become ready.
- `openpanels-local studio stop`: stop the conversation-local studio process.
- `openpanels-local agent bootstrap`: print the protocol v2 focus, state,
  capabilities, applicable guides, and active operations.
- `openpanels-local agent skills`: list loadable built-in skills.
- `openpanels-local agent skill <id>`: print one full workflow skill.
- `openpanels-local agent guide wiki.knowledge-context`: load the current
  CLI version's rules for using Wiki and raw-document knowledge context.
- `openpanels-local wiki selection read`: read whether the whole Wiki is
  selected and which raw documents the user selected directly.
- `openpanels-local wiki pages search`: search the selected Wiki space before
  reading relevant pages.
- `openpanels-local agent guides`: list loadable built-in guides.
- `openpanels-local agent guide <id>`: print one full workflow guide.
- `openpanels-local panel list`: list panels in the current Project.
- `openpanels-local panel current`: read the active Project panel.
- `openpanels-local panel switch`: switch the active Project panel.
- `openpanels-local wiki context`: read the current Wiki context.
- `openpanels-local canvas state`: read the current canvas session, panel, and state.
- `openpanels-local canvas selection read`: read selected shapes and optional PNG data.
- `openpanels-local canvas selection export`: write the exported selection PNG to a file.
- `openpanels-local canvas image insert`: add a local image file as a canvas image shape.

## Task Targets

OpenPanels only assigns background work to explicitly registered targets. A
target declares the capabilities it can execute, such as
`wiki.convertDocument` or `wiki.ingestMarkdown`.

Register a polling target and claim work atomically:

```bash
openpanels-local agent targets register \
  --name my-agent --transport poll \
  --capability wiki.ingestMarkdown --format json
openpanels-local tasks claim-next \
  --target-id <target-id> --wait-ms 25000 --format json
```

The claim response contains a lease token. Use it with `tasks heartbeat`,
`tasks complete`, `tasks fail`, or `tasks release`.

For a local command-based agent, the bridge owns this lifecycle automatically:

```bash
openpanels-local agent bridge \
  --name my-worker \
  --capability wiki.ingestMarkdown \
  --command '<agent command>'
```

Webhook targets register an endpoint and receive signed wake notifications.
The target must still claim the task before executing it. Use
`openpanels-local agent bridge status --format json` to inspect dispatcher,
target, retry, and running-task status.
