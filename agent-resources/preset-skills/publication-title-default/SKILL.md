---
name: publication-title-default
description: 根据正文内容和用户要求生成十个清晰、各有侧重的候选标题。
---

Generate exactly ten candidate titles from the captured publication content.

- Identify the article's central subject, reader value, and strongest concrete
  details before drafting titles.
- Follow the user's additional requirements for audience, tone, length, and
  style without introducing claims that the article does not support.
- Match the primary language used by the article unless the user explicitly
  requests another language.
- Make every candidate distinct in angle or phrasing. Avoid superficial word
  swaps, clickbait, vague slogans, and repeated existing titles.
- Keep each title concise enough for a publication headline. Do not add list
  numbers, surrounding quotation marks, explanations, subtitles, or metadata.
- Return exactly ten non-empty strings through the JSON artifact required by
  the runtime contract.
