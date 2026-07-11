# Use Canvas Selection As Reference

Use this reference when selected Canvas content is relevant to the task.

Workflow:

1. Read selection metadata from the CLI.
2. Use returned shapes, bounds, shape ids, and image metadata. Do not infer
   selection from screenshots.
3. If `isExplicitSelection` is false, stop when the request requires an explicit
   reference. Do not substitute the reported fallback.
4. Export selection pixels only when the operation needs them.
5. Carry available source information into generated image metadata, including
   shape id, asset ref, exported local path, and existing generation metadata.

A fallback such as the latest image may be useful context, but it is not a
user-confirmed selection.
