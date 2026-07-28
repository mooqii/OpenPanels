---
name: writing-distillation-default
description: Distill topic-neutral stylistic mechanics from examples into one portable Writing Skill that can be applied to any subject.
---

# Distill a Writing Skill

Create one reusable Writing Skill from the complete set of supplied examples.
Make it topic-neutral. Recover how the prose works, not what it discusses, who
it addresses, or what position it takes.

## Analyze the corpus

Read every example before drafting the Skill.

1. Separate authored prose from quotations, prompts, boilerplate, metadata,
   copied passages, and platform artifacts. Do not learn a style rule from text
   that is not part of the target writing.
2. Separate stable stylistic mechanics from changes caused by length, format,
   or formality. Preserve those changes only as optional presentation modes,
   never as topical or audience restrictions.
3. Build an internal evidence matrix across:
   - voice, narrative distance, point of view, and emotional temperature;
   - the shape of explanation, argument, description, and example placement;
   - document architecture, openings, development, transitions, and endings;
   - paragraph shape, sentence rhythm, pacing, and information density;
   - diction, register, recurring language functions, punctuation, and
     formatting;
   - rhetorical devices, emphasis, humor, uncertainty, and calls to action;
   - revision standards, stylistic failure modes, and textures the writing
     avoids.
4. Compare examples instead of summarizing them independently. Look for
   recurrence, meaningful variation, contradictions, and negative space.
5. Measure observable tendencies when the corpus supports them. Prefer honest
   ranges and relative frequencies over invented precision.

## Classify the evidence

Classify each candidate pattern before turning it into a rule:

- **Invariant**: repeated across the applicable examples or throughout one
  substantial example. Encode as a hard constraint only when violating it
  would clearly break the method.
- **Default**: dominant but not universal. Encode as the normal choice, with
  room for justified exceptions.
- **Conditional**: tied to length, format, formality, or rhetorical function.
  Encode the trigger and alternate behavior together without limiting subject
  matter.
- **Incidental**: isolated, topic-driven, or unsupported. Exclude it.

Treat absence as evidence only when the corpus is broad enough to make the
absence meaningful. Never convert "not observed" into "forbidden" by default.
When examples conflict, preserve the distinction as a selection rule instead
of averaging both approaches into vague advice.

## Distill executable rules

Turn evidence into decisions a writer can perform and an editor can check.

- Write each rule as an action plus its condition and observable result.
- Use `Must` sparingly for invariants, `Default to` for dominant tendencies,
  `When ...` for variants, and `Avoid` only for supported anti-patterns.
- Preserve relationships between techniques: what triggers a move, what it
  accomplishes, how often it appears, and what usually follows it.
- Capture how prose presents material: how it develops a point, places an
  example, controls emphasis, moves between sections, and varies density.
  Never prescribe which points, examples, opinions, or facts to use.
- Exclude source-specific subjects, audiences, purposes, facts, names, claims,
  opinions, values, expertise, personal details, and signature phrases from
  the rules. Preserve source language only in the compact evidence examples
  defined below.
- Do not add generic writing advice, fashionable anti-AI prohibitions, genre
  conventions, content methodology, or editorial positioning unless they are
  necessary to describe an observed stylistic mechanic.

If the material contains multiple stable subtypes, give the Skill a shared
core followed by conditional subtype rules. Do not blend incompatible voices
or document types into a fictional average.

## Attach evidence examples

Make a deep Writing Skill inspectable by placing compact evidence immediately
after each important conceptual summary or closely related rule cluster.

- Add an `Examples` block with one to three cases drawn from the authored prose
  that supports the concept.
- Use the shortest excerpt that still demonstrates the claimed mechanic. A
  phrase, sentence, paired sentences, or source-derived structural skeleton is
  usually enough.
- Preserve the evidence's syntax, punctuation, rhythm, paragraph break, or
  rhetorical move. Replace names, organizations, products, exact figures, and
  topic-specific nouns with neutral placeholders when they are not necessary
  to see the style.
- After each excerpt, state in one short line what it proves. Point to the
  observable feature rather than interpreting the source's subject matter.
- When an exact excerpt would expose identifying content or overwhelm the
  Skill, write a close, de-identified reconstruction and label it
  `Evidence-derived` rather than presenting it as a quotation.
- Use evidence from more than one example when available. Do not use the same
  attractive sentence to support unrelated concepts.
- Treat evidence as demonstration, not as a phrase bank. The generated Writing
  Skill must never instruct the writer to reuse the excerpt's topic, facts,
  opinions, imagery, or distinctive wording.

Evidence examples are allowed to be concrete because they verify the
distillation. Their role is to show the form of the writing while the rules
remain topic-neutral.

## Build the output Skill

Produce one self-contained `SKILL.md`.

The frontmatter must contain exactly:

```yaml
---
name: <requested-skill-id>
description: <use this style to write or revise prose on any subject>
---
```

Write the body in the language most useful for applying the Skill. Keep it
concise enough to load during every writing request, but specific enough to
produce a recognizable method. Prefer a deeper Skill with well-chosen evidence
over a short list of unsupported abstractions.

Organize the body around the evidence rather than a content or genre template.
It must make these decisions easy to find:

1. **Style signature**: a topic-neutral description of the prose's recognizable
   texture.
2. **Voice and delivery**: point of view, narrative distance, emotional
   temperature, formality, and degree of directness.
3. **Structure and movement**: opening behavior, progression, transitions,
   emphasis, paragraph rhythm, sentence rhythm, and endings.
4. **Language and rhetoric**: diction, syntax, punctuation, formatting,
   explanation patterns, and supported signature devices.
5. **Presentation modes**: conditional adjustments for length, format,
   formality, or rhetorical function without changing the underlying style.
6. **Draft and revision procedure**: how to apply the style to arbitrary input
   and remove stylistic drift.
7. **Guardrails and final check**: high-confidence stylistic failure modes and
   a short operational review checklist.

Immediately follow each high-value conceptual summary with its compact
`Examples` block. Keep evidence beside the concept it supports instead of
collecting all examples in a detached appendix.

State explicitly that new subject matter, facts, positions, examples, and
audiences come from the user's current request. The Skill supplies only the
writing style and must not import them from the examples.

Omit a section or dimension when the examples provide no defensible guidance.
Do not pad the Skill with empty categories.

## Validate before returning

Audit the finished Skill privately:

1. **Coverage**: every high-confidence pattern has an actionable rule.
2. **Traceability**: every strong rule can be justified by multiple examples,
   repeated evidence within a substantial example, or an explicit conditional
   pattern.
3. **Evidence quality**: every conceptual summary has a relevant, minimal
   example, and every example visibly demonstrates the feature claimed for it.
4. **Fidelity**: a new piece following the Skill would reproduce the examples'
   voice, structure, cadence, and linguistic texture without reproducing their
   subject matter.
5. **Contrast**: the Skill explains what distinguishes this method from a
   competent generic draft.
6. **Consistency**: hard rules, defaults, variants, and examples do not
   contradict one another.
7. **Calibration**: applying the Skill to a short, unfamiliar, neutral prompt
   would still produce its distinctive decisions without borrowing source
   content.
8. **Portability**: the package contains no source-file references,
   identifying content, external dependencies, host instructions, or lifecycle
   details. Only minimal de-identified evidence examples remain.
9. **Executability**: another agent can write and revise with the Skill without
   needing the original examples.

Revise any rule that is vague, unsupported, uncheckable, or overly literal.
If the supplied material contains no substantive authored prose or supports no
defensible stylistic pattern, report insufficient evidence instead of
inventing a style.
