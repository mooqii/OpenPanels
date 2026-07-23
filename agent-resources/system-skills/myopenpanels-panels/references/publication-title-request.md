# Request Publication Titles

Use this procedure to create a title-generation Task for a Typesetting
publication explicitly identified by the user or by a prior panel read.

1. List the available Title Skills and choose the best match. Prefer
   `publication-title-default` when the user did not request a specialized
   method.
2. Run `publication.title.generate` with the publication id, chosen Skill id,
   and the user's title requirements. Do not reinterpret or expand those
   requirements.
3. Report the created Task and stop. The Task Runtime owns title generation,
   validation, and publication updates.

Never execute the selected portable Skill directly from this procedure or add
candidate titles to panel state yourself.
