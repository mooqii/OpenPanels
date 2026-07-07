---
name: myopenpanels-open
description: Open MyOpenPanels for the active project through the openpanels-local CLI.
---

Use this skill when the user asks to open, view, or work in MyOpenPanels.

MyOpenPanels is controlled through the `openpanels-local` CLI. 

First check whether the CLI is installed:

```bash
command -v openpanels-local
```

If it is missing, use `npx -y @openpanels/local-cli@latest` in place of
`openpanels-local` for the commands below.

Start or reuse the local studio:

```bash
openpanels-local studio start --project "$PWD" --format json
```

Open the returned `serverUrl` for the user, or run:

```bash
openpanels-local studio open --project "$PWD" --format json
```

Use `openpanels-local studio status --project "$PWD" --format json` to inspect an
existing session, and `openpanels-local studio wait --project "$PWD" --timeout 10
--format json` after startup if you need to verify readiness.

Do not manually create or edit `.myopenpanels/` files.

The local studio stores state in the active project's `.myopenpanels/`
directory and syncs the current canvas selection there so agents can read it
later with `openpanels-local selection`.
