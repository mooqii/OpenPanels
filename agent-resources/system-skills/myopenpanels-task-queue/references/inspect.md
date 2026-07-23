# Inspect Project Tasks

Use this reference for read-only Task queue questions.

1. List or select the next Task using the user's queue and status criteria.
2. Read an exact Task to inspect its dependency, lease, result, error, and up to
   three embedded execution summaries.
3. Distinguish readiness from status: a queued Task may still be blocked by
   dependencies, retry timing, leases, or routing.
4. Do not claim, retry, cancel, archive, or mutate a Task during inspection.
