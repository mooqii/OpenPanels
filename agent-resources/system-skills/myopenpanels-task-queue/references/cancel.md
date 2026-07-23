# Cancel A Task

Use this reference only when the user explicitly asks to cancel an identified
Task.

1. Read the Task and verify its identity and current non-terminal state.
2. Run the high-risk `task.cancel` command once.
3. Report the resulting terminal state. Do not interpret cancellation as a
   failed execution or silently cancel dependent Tasks.
