import { readdir, readFile } from "node:fs/promises"
import { extname, join, relative } from "node:path"

const ROOTS = ["apps", "crates", "scripts"]
const SOURCE_EXTENSIONS = new Set([".css", ".js", ".mjs", ".rs", ".ts", ".tsx"])
const IGNORED_DIRECTORIES = new Set(["dist", "node_modules", "target"])
const WARNING_LINE_COUNT = 800
const MAX_LINE_COUNT = 1000

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
  const result = { file: relative(process.cwd(), file), lineCount }
  if (lineCount > MAX_LINE_COUNT) oversized.push(result)
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
      `error: ${result.file} has ${result.lineCount} lines (maximum: ${MAX_LINE_COUNT})`
    )
  }
  process.exitCode = 1
} else {
  console.log(
    `Source file length check passed (${files.length} files scanned).`
  )
}
