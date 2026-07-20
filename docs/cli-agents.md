# MyOpenPanels CLI For Agents

Resolve one CLI executable and preserve it for the entire request. Start Studio
with `studio start --local-only --project-dir "$PWD" --format json`. Execute the
returned top-level `actions.required` in order: try the URL opener first and use
the conditional CLI fallback only when that opener fails or is unavailable.

Before panel work with a clear intent, run `agent bootstrap --workflow <key>
--format json`. It provides visible focus, the non-activating target, Workflow
readiness, relevant context, required Skill and reference reads, and only the
registered command descriptions needed for that Workflow. Use generic `agent
bootstrap --format json` only when intent is ambiguous or no supported Workflow
key is known. Execute `actions.required` sequentially, then choose
`actions.suggested` by their structured conditions. Do not reuse an earlier
Bootstrap result.

Use `agent catalog` for the domain index and `agent catalog --domain <domain>`
for complete command definitions after generic discovery. Workflow Bootstrap
does not require a subsequent Catalog call: instantiate only its returned
`commands.items` descriptors and never reconstruct commands from prose or
memory.

Claimed Task Broker execution and Studio-generated `task scope read` handoffs do
not run Bootstrap. Preserve their exact selector, lease, fencing, and Runtime
Contract.

All JSON responses use Envelope v3 with `ok`, `schemaVersion`, `intent`, either
`data` or `error`, `actions`, and `meta`. Error recovery is represented by the
same top-level action arrays. `actions.required` is ordered; suggested actions
are optional and conditional.

Reads and writes can target any Project panel without changing focus. `panel
selection read` is deliberately different: it reads only the selection in the
currently active panel. High-risk writes are labeled in catalog metadata but do
not require a second confirmation.
