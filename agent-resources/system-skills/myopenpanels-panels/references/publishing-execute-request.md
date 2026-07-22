# Execute A Publishing Request

Use this reference only inside the exact claimed Publishing Task Handoff or
Agent CLI execution.

1. Require every bound identifier, input path, checkpoint command, workspace
   path, and result field supplied by the ExecutionBundle. Do not reconstruct a
   missing value from panel state or another Task.
2. Read the captured title, body, ordered media, and complete portable
   Publishing Skill from their bound inputs.
3. Use the permitted browser destination and preserve the captured content
   verbatim. Stop with `needs_user_action` when authentication, verification,
   account confirmation, or browser availability blocks progress.
4. Run the pre-bound `prepared` checkpoint after validating the populated
   form, and the pre-bound `committing` checkpoint immediately before the one
   final platform action.
5. Write the declared ExecutionResult exactly once with `published`,
   `needs_user_action`, `not_published`, or `unknown` and the matching reason
   and observation fields.
6. In Agent CLI mode, leave lifecycle completion to the Runtime. In Agent
   Message mode, use only the exact heartbeat, complete, fail, or stop commands
   returned in the Delivery Contract.

Never reuse a release snapshot, command, workspace, or result path from another
Attempt.
