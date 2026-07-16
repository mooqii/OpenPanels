# CLI Error Contract

All structured CLI failures use Envelope v3:

```json
{
  "ok": false,
  "schemaVersion": 3,
  "intent": "wiki.page.read",
  "error": {
    "type": "not_found",
    "subtype": "wiki_page_not_found",
    "message": "Wiki page not found",
    "retryable": false,
    "hint": "Check the page path and retry."
  },
  "actions": {
    "required": [],
    "suggested": []
  },
  "meta": {
    "cliVersion": "0.4.11"
  }
}
```

`error.type` is the broad category and `error.subtype` is the stable domain
reason. There is no duplicate error-code field. Recovery commands are typed
top-level actions, not nested display strings.

Exit codes remain category based: validation and usage errors use `2`, ordinary
domain failures use `1`, conflicts use `3`, and unavailable host services use
`4`. `retryable` and action metadata, rather than the exit code alone, determine
whether an Agent should retry.
