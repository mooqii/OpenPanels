---
id: writing-skill-refiner
title: 提炼写作
description: 从用户选中的文章中归纳可复用的写作风格、结构与表达技巧，并生成当前项目可用的 Writing Skill。
source: builtin
appliesTo:
  - writing
taskTypes:
  - refine_writing_skill
requiresCapabilities:
  - writing.refinement.read
  - writing.skill.install
  - wiki.raw-document.markdown.read
  - wiki.generated-document.read
  - task.complete
  - task.fail
loadWhen:
  - The submitted Writing task refines selected articles into a project Writing Skill.
tokens: short
---

Create one reusable Writing Skill from every source captured by the refinement
task.

1. Read the immutable refinement request. Read every selected raw-document
   Markdown file and generated document; never search or read Wiki pages.
2. Analyze the sources together. Extract repeatable guidance about audience,
   voice, tone, structure, pacing, paragraph and sentence patterns, rhetorical
   techniques, formatting, and quality checks.
3. Exclude source-specific subjects, facts, names, quotations, claims, and
   personal details. Convert useful examples into abstract patterns rather than
   copying distinctive passages.
4. Produce one self-contained `SKILL.md` with the exact id and title captured by
   the task. Its frontmatter must use `source: project`, `appliesTo: writing`,
   `taskTypes: generate_document`, and an empty `requiresCapabilities` list.
5. Give the Skill a concise description and actionable authoring rules. Do not
   reference other files or the source documents from the finished Skill.
6. Install the file with `writing skill install`, then complete the Task. Fail
   the Task when a source is unavailable, the generated Skill is invalid, or a
   name conflict is reported.

The finished Skill should reproduce the sources' reusable writing method, not
their subject matter.
