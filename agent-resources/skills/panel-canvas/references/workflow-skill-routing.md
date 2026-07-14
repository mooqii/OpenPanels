# Route Canvas Generation To A Workflow Skill

The Canvas panel skill owns selection, target capture, operation lifecycle,
placement, and result metadata. A separate drawing workflow skill may own
artistic direction, prompt construction, style, and model-specific generation
steps.

- When Agent Bootstrap or the generation operation advertises a non-empty
  `workflowSkillId`, load that skill before invoking the image model and follow
  its task-relevant references.
- When no workflow skill is selected, continue with the Canvas panel's general
  image-generation contract.
- A workflow skill must not bypass Canvas generation begin/complete, replace the
  captured target, reinterpret fallback content as selection, or omit required
  result metadata.

The current protocol does not expose a Canvas workflow selector, so
`workflowSkillId` is normally absent. This contract is reserved for compatible
future workflow skills.
