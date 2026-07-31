# Execute A Publishing Request

Use this reference only inside the exact claimed Agent Message Publishing Task
Handoff. Publishing Tasks do not support Agent CLI execution.

1. Require every bound identifier, input path, checkpoint command, workspace
   path, and result field supplied by the ExecutionBundle. Do not reconstruct a
   missing value from panel state or another Task.
2. Read the captured title, body, ordered media, and complete portable
   Publishing Skill from their bound inputs.
3. Use only the permitted browser destination or exact fenced official API
   command declared by the Runtime Contract. Preserve captured content verbatim
   unless that Runtime Contract explicitly authorizes derived fields or minimal
   adaptations for platform limits. Stop with `needs_user_action` when required
   account configuration, authentication, verification, or platform permission
   blocks progress.
4. Run the pre-bound `prepared` checkpoint after validating the populated form
   or immutable API inputs, and the pre-bound `committing` checkpoint
   immediately before the one final platform action.
5. Write the declared ExecutionResult exactly once with `published`,
   `needs_user_action`, `not_published`, or `unknown` and the matching reason
   and observation fields.
6. Use only the exact heartbeat, complete, fail, or stop commands returned in
   the Agent Message Delivery Contract.

Never reuse a release snapshot, command, workspace, or result path from another
Attempt.
