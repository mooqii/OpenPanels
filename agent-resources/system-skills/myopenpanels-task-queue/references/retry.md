# Retry A Failed Task

Use this reference only when the user explicitly asks to retry an identified
failed Task.

1. Read the Task and confirm that retry is valid for its current terminal or
   failed state.
2. Run `task.retry` once using the exact Task id.
3. Report the resulting status and readiness. Do not claim or execute the Task
   unless a separate Studio scope handoff is supplied.
