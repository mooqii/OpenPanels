import { readFileSync, writeFileSync } from "node:fs"
import {
  renderCapabilityMatrix,
  renderEntryCapabilityIndex,
  replaceCapabilityMatrix,
  replaceEntryCapabilityIndex,
} from "./lib/capability-projections.mjs"

const ROOT = new URL("..", import.meta.url)
const catalogUrl = new URL(
  "agent-resources/module-capability-catalog.json",
  ROOT
)
const entrySkillUrl = new URL("skills/myopenpanels/SKILL.md", ROOT)
const capabilityDocUrl = new URL("docs/module-capabilities.md", ROOT)
const catalog = JSON.parse(readFileSync(catalogUrl, "utf8"))
const entrySkill = readFileSync(entrySkillUrl, "utf8")
const rendered = renderEntryCapabilityIndex(catalog)
const updated = replaceEntryCapabilityIndex(entrySkill, rendered)
const capabilityDoc = readFileSync(capabilityDocUrl, "utf8")
const updatedCapabilityDoc = replaceCapabilityMatrix(
  capabilityDoc,
  renderCapabilityMatrix(catalog)
)

if (updated !== entrySkill) {
  writeFileSync(entrySkillUrl, updated)
}
if (updatedCapabilityDoc !== capabilityDoc) {
  writeFileSync(capabilityDocUrl, updatedCapabilityDoc)
}
