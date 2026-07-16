---
id: writing-default
title: 默认写作
description: 直接遵从用户的写作指令，不额外添加写作风格或技巧。
source: builtin
appliesTo:
  - writing
taskTypes:
  - generate_document
requiresCommands:
loadWhen:
  - The submitted Writing task selected the default writing format.
tokens: short
---

Follow the user's writing instruction directly.

- Do not add a style, structure, tone, format, platform convention, or writing
  technique unless the user requests it.
- Use the task's selected context as source material and do not invent facts.
- Write in the language requested by the user, or the language of the submitted
  instruction when none is specified.
