# Agent Guidance Protocol v6

Protocol v6 uses two stable entries:

1. `agent bootstrap --format json` returns current user-visible context and the
   required/suggested actions for this request.
2. `agent catalog [--domain <domain>] --format json` returns the command domain
   index or one complete domain catalog.

Every response uses Envelope v3. The top-level shape always includes `ok`,
`schemaVersion`, `intent`, `actions`, and `meta`, plus exactly one of `data` or
`error`. Actions are typed. CLI actions contain `intent`, `executor: "cli"`, and
an `argv` array; file reads, URL opens, and Skill installation use explicit action
kinds rather than shell command strings.

Execute `actions.required` in array order. Evaluate `actions.suggested` only
after all required actions succeed, using each action's structured condition.
Studio startup returns a required URL action followed by a conditional CLI
fallback action.

Bootstrap remains within 8192 UTF-8 bytes. It includes compact context and Skill
references, while each Skill's `requiresCommands` declarations generate domain
catalog actions. The CLI registry, rather than Skill prose, defines command
syntax.

Only the active selection is focus-bound. Other reads and writes target explicit
resources or panel kinds without requiring or changing the active panel.
