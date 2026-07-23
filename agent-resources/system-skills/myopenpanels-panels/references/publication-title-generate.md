# Generate Publication Titles

Use this reference only inside an exact claimed `generate_publication_titles`
Task.

1. Require the bound Task, publication snapshot, Title Skill, workspace, input,
   output, and result parameters from the ExecutionBundle. Do not reconstruct a
   missing value from current panel state.
2. Read the selected title, all existing titles, article body, complete Title
   Skill package, and additional requirements from their bound inputs.
3. Generate one or more distinct candidate titles grounded in the captured
   content, using the captured Title Skill to determine the candidate count. Do
   not repeat an existing title or add facts absent from the source.
4. Write exactly one `publication-titles` JSON artifact with a non-empty `titles`
   array of non-empty strings, then write the exact ExecutionResult at its bound
   path.
5. Do not add titles to Typesetting state yourself. The Runtime validates and
   appends the candidates to the captured publication before completing the
   Task.

Never number candidates, include explanations in the title strings, or reuse an
artifact from another Attempt.
