# Manage Wiki Spaces

Use this reference to list Wiki spaces, make a space active, or materialize its
generated pages for local reading.

- Use `wiki.space.list` before acting when the user did not provide an exact
  space id.
- Use `wiki.space.activate` only when the user explicitly asks to change the
  active Wiki space.
- Use `wiki.space.materialize` when local access is `on_demand`; consume only a
  returned `ready` root and manifest.
- Materialization is read preparation. It must not rewrite Wiki content or
  change the visible panel.
