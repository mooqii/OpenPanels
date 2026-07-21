---
name: typesetting-cover-default
description: 根据文章标题和正文主题生成简洁、清晰的编辑视觉封面。
---

Create one editorial cover image from the captured article title and body.

- Extract the central subject, mood, and one or two concrete visual motifs from
  the article. Do not merely illustrate the first sentence.
- Prefer a clean landscape composition close to 4:3, with a strong focal point,
  useful negative space, and enough contrast to survive thumbnail cropping.
- Do not render words, letters, logos, watermarks, UI, or fake publication marks
  unless the user's additional requirements explicitly request typography.
- Avoid generic stock-photo staging, decorative gradients, and unrelated
  atmospheric imagery.
- Follow the user's additional requirements for style and subject while keeping
  the article snapshot as the factual source.
- Generate a real PNG bitmap and save exactly one cover artifact at the path
  required by the runtime contract.
