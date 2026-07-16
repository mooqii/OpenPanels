# MyOpenPanels CLI For Agents

Resolve one CLI executable and preserve it for the entire request. Start Studio
with `studio start --local-only --project-dir "$PWD" --format json`. Execute the
returned top-level `actions.required` in order: try the URL opener first and use
the conditional CLI fallback only when that opener fails or is unavailable.

Before panel work, run `agent bootstrap --format json`. Bootstrap provides the
current Project, active panel and selection summary, required Skills, and
structured actions. Execute `actions.required` sequentially, then choose
`actions.suggested` by their structured conditions. Do not reuse an earlier
Bootstrap result.

Use `agent catalog` for the domain index and `agent catalog --domain <domain>`
for complete command definitions. Execute only returned `argv` arrays. Do not
reconstruct commands from prose or memory.

All JSON responses use Envelope v3 with `ok`, `schemaVersion`, `intent`, either
`data` or `error`, `actions`, and `meta`. Error recovery is represented by the
same top-level action arrays. `actions.required` is ordered; suggested actions
are optional and conditional.

Reads and writes can target any Project panel without changing focus. `panel
selection read` is deliberately different: it reads only the selection in the
currently active panel. High-risk writes are labeled in catalog metadata but do
not require a second confirmation.
