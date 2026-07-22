# MyOpenPanels CLI For Agents

Resolve one CLI executable and preserve it for the entire request. Start Studio
with `studio start --local-only --project-dir "$PWD" --format json`. Execute the
returned top-level `actions.required` in order: try the URL opener first and use
the conditional CLI fallback only when that opener fails or is unavailable.

Studio-generated Agent instructions are runtime-bound. Development Studio
responses advertise the checkout-local CLI from `MYOPENPANELS_CLI`; release
Studio responses advertise the installed `myopenpanels`. Preserve that exact
executable for every returned `argv` action and never cross from one channel to
the other, because their storage and running Studio sessions are intentionally
separate.

Before panel work with a clear intent, run `agent bootstrap --procedure <key>
--format json`. It provides visible focus, the non-activating target, Procedure
readiness, relevant context, required Skill and reference reads, and only the
registered command descriptions needed for that Procedure. Use generic `agent
bootstrap --format json` only when intent is ambiguous or no supported Procedure
key is known. Execute `actions.required` sequentially, then choose
`actions.suggested` by their structured conditions. Do not reuse an earlier
Bootstrap result.

Use `agent catalog` for the domain index and `agent catalog --domain <domain>`
for complete command definitions after generic discovery. Procedure Bootstrap
does not require a subsequent Catalog call: instantiate only its returned
`commands.items` descriptors and never reconstruct commands from prose or
memory.

Claimed Task Broker execution and Studio-generated `task handoff start` Task
Handoffs do not run Bootstrap. A Handoff returns ExecutionBundle v2 and a
Delivery Contract with bound `exec`, heartbeat, completion, failure, and stop
actions. The Bundle inlines its required System References, captured Skill,
workspace paths, identifiers, and work-command parameters so both Agent Message
and automatic Agent CLI execution can submit one complete instruction set to
the model. Do not perform separate Catalog or Skill discovery and do not use
low-level Task lifecycle commands. Write only the declared workspace artifacts;
the Runtime creates Operations, stages content, and completes the Task. The
automatic Agent CLI uses the same Bundle, TaskOutputPlan, and Finalizer; only
its Bridge-managed Delivery Contract differs.

Automatic Agent targets advertise only capabilities owned by the static Task
Handler Registry. Unregistered queue/type/capability tuples are not routed and
cannot fall back to generic Catalog-driven execution. Runtime Finalization moves
through `validating`, `applying`, `committing`, and `completed`; failures expose
the failed phase through the shared Finalizer result.

Workflow Runs are durable Task DAG executions, not Procedure routes. Inspect
them with `workflow run list` and `workflow run read`; their public identity is
`workflowRunId`, and `definitionKey` identifies the kind of process being run.

All JSON responses use Envelope v3 with `ok`, `schemaVersion`, `intent`, either
`data` or `error`, `actions`, and `meta`. Error recovery is represented by the
same top-level action arrays. `actions.required` is ordered; suggested actions
are optional and conditional.

Reads and writes can target any Project panel without changing focus. `panel
selection read` is deliberately different: it reads only the selection in the
currently active panel. High-risk writes are labeled in catalog metadata but do
not require a second confirmation.
