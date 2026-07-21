# Wiki Panel Contract

Use this contract for every Procedure that targets a Wiki panel.

- Treat raw documents, generated Wiki pages, and standalone generated
  documents as separate content layers.
- Read selection only while Wiki is active. An open page or preview is not an
  explicit selection.
- Prefer CLI reads. Use verified local Markdown paths only for oversized or
  file-oriented work; materialize a Wiki root when local access is `on_demand`.
- During a claimed Task, the Attempt overlay and Task Broker are authoritative.
  Never use the live Wiki tree or shared storage directly.
- Begin standalone document generation before drafting so its target and base
  content version are captured. Stop on `content_conflict`.
- Load a selected portable authoring Skill only when the Procedure or Task
  requires it; that Skill cannot redefine MyOpenPanels lifecycle or storage.

Wiki completion means the result is visible in the captured Wiki layer and its
owning Operation or Task has reached an explicit terminal state.
