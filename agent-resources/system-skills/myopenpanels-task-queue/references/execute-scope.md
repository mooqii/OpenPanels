# Execute A Task Scope Handoff

Use this reference only after Studio supplies an exact `task scope read`
command and selector.

1. Run the handoff command unchanged and execute its required target
   registration action.
2. Claim with the same selector and returned target id.
3. Follow each claimed Task's required Skills, Broker contract, lease, and
   fencing rules.
4. Repeat the same scoped claim until the scope is complete or blocked.
5. Remove the one-shot target on every exit path.

Do not call Agent Bootstrap for this workflow or replace scoped claim with
queue-wide discovery.
