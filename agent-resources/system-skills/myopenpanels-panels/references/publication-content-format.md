# Format Typesetting Content

Use this reference only inside an exact claimed `format_publication_content`
Task.

1. Require the bound Task, request, publication, Layout Skill, workspace, input,
   output, and result parameters from the ExecutionBundle. Do not reconstruct a
   missing value from current panel state.
2. Read the captured title, complete TipTap JSON document, complete Layout Skill
   package, and additional requirements from their bound inputs.
3. Follow the captured Layout Skill and additional requirements for both
   editorial and layout changes. Keep the article's subject and intent intact,
   preserve every link target and every image with all attributes, and use only
   supported TipTap structure and marks.
4. Write exactly one valid UTF-8 `publication-content` JSON artifact at the
   declared path and the exact ExecutionResult at its declared path.
5. Do not replace publication content yourself. The Runtime validates the
   document structure and preserved resources, checks the captured content
   version, commits the result, and completes the Task.

Stop on invalid input, unsupported schema, or content conflict.
