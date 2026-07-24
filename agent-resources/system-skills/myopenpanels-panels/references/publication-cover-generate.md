# Generate A Publication Cover

Use this reference only inside an exact claimed `generate_publication_cover`
Task.

1. Require the bound Task, request, publication, Cover Skill, workspace, input,
   output, and result parameters from the ExecutionBundle. Do not reconstruct a
   missing value from current panel state.
2. Read the captured title, body, complete Cover Skill package, and additional
   requirements from their bound inputs.
3. Use an available image-generation tool to create one or more real PNG bitmaps. Do not
   substitute SVG, HTML, a manually scripted drawing, or an unrelated image.
4. Write each non-empty PNG as a separate `publication-cover` artifact under
   `outputs/`, using `outputs/cover.png` first and `outputs/cover-2.png`,
   `outputs/cover-3.png`, and so on for alternatives. Write the exact
   ExecutionResult at its declared path.
5. Do not add the cover to Typesetting state yourself. The Runtime validates
   every PNG, stores it, links it to the captured publication, and completes the
   Task.

Never create contact sheets, mockups, non-PNG variants, or reuse an artifact
from another Attempt.
