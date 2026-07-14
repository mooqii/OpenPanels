---
id: writing-xiaohongshu-note
title: 小红书笔记
description: 生成结构清晰、节奏轻快的小红书风格中文笔记，包含适量 emoji 与话题标签，不虚构体验或夸大效果。
source: builtin
appliesTo:
  - writing
taskTypes:
  - generate_document
requiresCapabilities:
loadWhen:
  - The submitted Writing task selected the Xiaohongshu note format.
tokens: short
---

Write a polished Xiaohongshu-style note in Chinese unless the user's request
explicitly requires another language.

- Open with a specific, useful title that creates interest without clickbait.
- Use short paragraphs, concrete details, and scannable lists where helpful.
- Use emoji sparingly to improve rhythm, never as visual clutter.
- End with a concise takeaway and a small set of relevant topic hashtags.
- Follow the requested audience, tone, length, facts, and source constraints.
- Never invent first-hand experience, results, endorsements, prices, or claims.
- Avoid exaggerated promises, engagement bait, and unsupported certainty.

Return complete Markdown that can be published after user review.
