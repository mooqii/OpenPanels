# Format Typesetting Content

Use this reference only inside an exact claimed `format_typesetting_content`
Task.

1. Require the bound Task, request, publication, Layout Skill, workspace, input,
   output, and result parameters from the ExecutionBundle. Do not reconstruct a
   missing value from current panel state.
2. Read the captured title, complete TipTap JSON document, complete Layout Skill
   package, and additional requirements from their bound inputs.
3. Preserve every text character in order, every link target and range, and
   every image with all attributes. Change only supported TipTap structure and
   bold or italic emphasis allowed by the Runtime Contract.
4. Write exactly one valid UTF-8 `typesetting-content` JSON artifact at the
   declared path and the exact ExecutionResult at its declared path.
5. Do not replace publication content yourself. The Runtime validates semantic
   preservation, checks the captured content version, commits the result, and
   completes the Task.

Stop on invalid input, unsupported schema, or content conflict; never produce a
best-effort rewrite.
