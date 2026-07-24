import { readdir, readFile } from "node:fs/promises"
import { extname, join, relative } from "node:path"

const ROOTS = ["apps", "crates", "scripts"]
const SOURCE_EXTENSIONS = new Set([".css", ".js", ".mjs", ".rs", ".ts", ".tsx"])
const IGNORED_DIRECTORIES = new Set(["dist", "node_modules", "target"])
const WARNING_LINE_COUNT = 800
const MAX_LINE_COUNT = 1000
const MAX_LINE_COUNT_OVERRIDES = new Map([
  ["apps/studio/src/App.tsx", 1032],
  ["apps/studio/src/components/typesetting/TypesettingPublication.tsx", 1415],
  ["apps/studio/src/components/wiki/useWikiPanelController.tsx", 1013],
  ["apps/studio/src/styles/typesetting.css", 1436],
  ["crates/myopenpanels/src/bridge/result_validation.rs", 1396],
  ["crates/myopenpanels/src/bridge/task_handlers.rs", 1249],
  ["crates/myopenpanels/src/agent/skill_import.rs", 1011],
  ["crates/myopenpanels/src/cli/tests/bootstrap_and_parsing.rs", 1033],
  ["crates/myopenpanels/src/content/filesystem.rs", 1702],
  ["crates/myopenpanels/src/control/runtime.rs", 1096],
  ["crates/myopenpanels/src/release.rs", 1183],
  ["crates/myopenpanels/src/studio/lifecycle.rs", 1037],
  ["crates/myopenpanels/src/writing/tests.rs", 1045],
])

async function sourceFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true })
  const files = []
  for (const entry of entries) {
    if (entry.name.startsWith(".") || IGNORED_DIRECTORIES.has(entry.name))
      continue
    const path = join(directory, entry.name)
    if (entry.isDirectory()) files.push(...(await sourceFiles(path)))
    else if (SOURCE_EXTENSIONS.has(extname(entry.name))) files.push(path)
  }
  return files
}

const files = (await Promise.all(ROOTS.map(sourceFiles))).flat()
const oversized = []
const warnings = []

for (const file of files) {
  const text = await readFile(file, "utf8")
  const lineCount = text.length === 0 ? 0 : text.split("\n").length
  const relativeFile = relative(process.cwd(), file)
  const maximum = MAX_LINE_COUNT_OVERRIDES.get(relativeFile) ?? MAX_LINE_COUNT
  const result = { file: relativeFile, lineCount, maximum }
  if (lineCount > maximum) oversized.push(result)
  else if (lineCount > WARNING_LINE_COUNT) warnings.push(result)
}

for (const result of warnings.sort((a, b) => b.lineCount - a.lineCount)) {
  console.warn(
    `warning: ${result.file} has ${result.lineCount} lines (target: <= ${WARNING_LINE_COUNT})`
  )
}

if (oversized.length > 0) {
  for (const result of oversized.sort((a, b) => b.lineCount - a.lineCount)) {
    console.error(
      `error: ${result.file} has ${result.lineCount} lines (maximum: ${result.maximum})`
    )
  }
  process.exitCode = 1
} else {
  console.log(
    `Source file length check passed (${files.length} files scanned).`
  )
}
