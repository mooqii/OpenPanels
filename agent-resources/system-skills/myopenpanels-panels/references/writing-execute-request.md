# Execute A Writing Request

Use this reference only inside the exact claimed `write_my_document` Task
handoff.

1. Read the immutable request with `writing.request.read`.
2. Load the task-selected Writing Skill and the relevant captured sources.
3. In revision mode, read the captured target document.
4. Begin the task-bound `writing.write` Operation before drafting.
5. Write the complete result, complete the Operation, then complete the Task.
6. Fail explicitly on model, source, target, or content-version errors.

Never replace the Task handoff with Agent Bootstrap or write directly to shared
storage.
