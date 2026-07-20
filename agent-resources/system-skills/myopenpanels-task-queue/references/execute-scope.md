# Execute A Task Scope Handoff

Use this reference only after Studio supplies an exact `task handoff start`
command and selector.

1. Run the handoff command unchanged. It registers the one-shot target, claims
   the first execution unit, and returns ExecutionBundle v2.
2. Follow the returned Delivery Contract, write only its declared workspace
   artifacts and `execution-result.json`, and use the bound `task handoff exec`
   runner only for allowed reads or Publishing checkpoints. The Runtime owns
   Operation creation and output staging.
3. Finish through the returned `task handoff complete` or `task handoff fail`
   action. The Runtime advances the same scope and returns the next Bundle.
4. Continue until `scopeState` is `complete` or `blocked`. Use
   `task handoff stop` when abandoning the handoff.

Do not call Agent Bootstrap, Catalog discovery, Skill discovery, low-level Task
lifecycle commands, or replace the supplied scope selector with queue-wide
discovery.
