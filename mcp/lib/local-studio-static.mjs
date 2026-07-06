import { spawn } from "node:child_process"
import { createHash } from "node:crypto"
import { existsSync, readFileSync } from "node:fs"
import { mkdir, readdir, readFile, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join, relative, resolve, sep } from "node:path"
import { fileURLToPath } from "node:url"

const BUILD_MARKER_FILE = ".openpanels-local-studio-build.json"
let cachedStaticHtml = ""
let pendingStaticHtml = null

export async function localStudioStaticHtml() {
  if (cachedStaticHtml) return cachedStaticHtml

  pendingStaticHtml ??= buildLocalStudioStaticHtml().finally(() => {
    pendingStaticHtml = null
  })
  cachedStaticHtml = await pendingStaticHtml
  return cachedStaticHtml
}

async function buildLocalStudioStaticHtml() {
  await ensureStaticBuildDir()
  return inlineViteBuild(staticBuildDir())
}

async function ensureStaticBuildDir() {
  await mkdir(staticBuildDir(), { recursive: true })
  const sourceHash = await buildSourceHash()
  if (existsSync(join(staticBuildDir(), "index.html"))) {
    const marker = await readBuildMarker()
    if (marker?.sourceHash === sourceHash) return
  }

  await runViteBuild()
  await writeBuildMarker(sourceHash)
}

function runViteBuild() {
  return runCommand(
    process.execPath,
    [viteCliPath(), "build", "--outDir", staticBuildDir(), "--emptyOutDir"],
    {
      cwd: localStudioDir(),
      failureLabel: "Vite build failed while preparing MyOpenPanels widget",
    }
  )
}

function runCommand(command, args, { cwd, env = {}, failureLabel }) {
  return new Promise((resolveRun, reject) => {
    const logs = []
    const child = spawn(command, args, {
      cwd,
      env: {
        ...process.env,
        ...env,
        BROWSER: "none",
        FORCE_COLOR: "0",
      },
      stdio: ["ignore", "pipe", "pipe"],
    })

    const capture = (chunk) => {
      logs.push(String(chunk))
      if (logs.length > 120) logs.shift()
    }
    child.stdout?.on("data", capture)
    child.stderr?.on("data", capture)

    child.once("error", reject)
    child.once("exit", (code, signal) => {
      if (code === 0) {
        resolveRun()
        return
      }
      reject(
        new Error(
          `${failureLabel} (${signal || `code ${code}`}).\n${logs.join("")}`
        )
      )
    })
  })
}

async function readBuildMarker() {
  try {
    return JSON.parse(
      await readFile(join(staticBuildDir(), BUILD_MARKER_FILE), "utf8")
    )
  } catch (_error) {
    return null
  }
}

async function writeBuildMarker(sourceHash) {
  await writeFile(
    join(staticBuildDir(), BUILD_MARKER_FILE),
    `${JSON.stringify({ sourceHash }, null, 2)}\n`
  )
}

async function buildSourceHash() {
  const hash = createHash("sha256")
  const sourceFiles = [
    join(pluginRootDir(), ".codex-plugin", "plugin.json"),
    join(pluginRootDir(), "package.json"),
    join(pluginRootDir(), "pnpm-lock.yaml"),
    join(localStudioDir(), "index.html"),
    join(localStudioDir(), "package.json"),
    join(localStudioDir(), "vite.config.ts"),
    ...(await listFiles(join(localStudioDir(), "src"))),
    ...(await listFiles(join(pluginRootDir(), "packages", "canvas", "src"))),
    ...(await listFiles(join(pluginRootDir(), "packages", "protocol", "src"))),
  ].sort()

  for (const file of sourceFiles) {
    hash.update(relative(pluginRootDir(), file))
    hash.update(await readFile(file))
  }

  return hash.digest("hex")
}

async function listFiles(root) {
  const entries = await readdir(root, { withFileTypes: true })
  const files = []

  for (const entry of entries) {
    const fullPath = join(root, entry.name)
    if (entry.isDirectory()) {
      files.push(...(await listFiles(fullPath)))
    } else if (entry.isFile()) {
      files.push(fullPath)
    }
  }

  return files
}

async function inlineViteBuild(outDir) {
  let html = await readFile(join(outDir, "index.html"), "utf8")
  const inlineScripts = []
  const consumedAssets = new Set()

  html = html.replace(
    /<link\s+rel="modulepreload"[^>]+href="([^"]+)"[^>]*>\s*/g,
    ""
  )

  html = await replaceAsync(
    html,
    /<link\s+rel="stylesheet"[^>]+href="([^"]+)"[^>]*>/g,
    async (_match, href) => {
      const css = await readBuildAsset(outDir, href, consumedAssets)
      return `<style>\n${escapeInlineStyle(css)}\n</style>`
    }
  )

  html = await replaceAsync(
    html,
    /<script\s+type="module"[^>]+src="([^"]+)"[^>]*><\/script>/g,
    async (_match, src) => {
      const js = await readBuildAsset(outDir, src, consumedAssets)
      inlineScripts.push(
        `<script>\n(() => {\n${escapeInlineScript(js)}\n})();\n</script>`
      )
      return ""
    }
  )

  if (inlineScripts.length > 0) {
    const scripts = inlineScripts.join("\n")
    html = html.includes("</body>")
      ? html.replace("</body>", () => `${scripts}\n</body>`)
      : `${html}\n${scripts}`
  }

  await assertNoUnconsumedBuildAssets(outDir, consumedAssets)

  assertCspCompatibleStaticHtml(html)
  return html
}

async function assertNoUnconsumedBuildAssets(outDir, consumedAssets) {
  const assetsDir = join(outDir, "assets")
  if (!existsSync(assetsDir)) return

  const leftovers = (await readdir(assetsDir)).filter(
    (name) => !consumedAssets.has(`assets/${name}`)
  )
  if (leftovers.length > 0) {
    throw new Error(
      `The MyOpenPanels widget build emitted non-inlined assets: ${leftovers.join(", ")}`
    )
  }
}

export function assertCspCompatibleStaticHtml(html) {
  const shellMarkup = html
    .replace(/<script\b[\s\S]*?<\/script>/gi, "")
    .replace(/<style\b[\s\S]*?<\/style>/gi, "")

  const forbiddenShellPatterns = [
    [/<script\b[^>]+\bsrc\s*=/i, "external script tag"],
    [/<script\b[^>]*\btype\s*=\s*["']module["']/i, "module script tag"],
    [/<link\b[^>]+\bhref\s*=/i, "external link tag"],
    [/<iframe\b/i, "iframe tag"],
    [/<(?:object|embed|base)\b/i, "embedded/base tag"],
  ]
  for (const [pattern, label] of forbiddenShellPatterns) {
    if (pattern.test(shellMarkup)) {
      throw new Error(
        `The MyOpenPanels widget is not CSP-compatible: found ${label}.`
      )
    }
  }

  for (const value of resourceAttributeValues(shellMarkup)) {
    if (isExternalResourceValue(value)) {
      throw new Error(
        `The MyOpenPanels widget is not CSP-compatible: found external resource ${value}.`
      )
    }
  }
}

function resourceAttributeValues(markup) {
  return Array.from(
    markup.matchAll(/\b(?:src|href)\s*=\s*(["'])(.*?)\1/gi),
    (match) => match[2].trim()
  )
}

function isExternalResourceValue(value) {
  if (!value) return false
  if (/^(?:#|data:|blob:|about:blank\b)/i.test(value)) return false
  return /^(?:[a-z][a-z0-9+.-]*:|\/\/|\/|\.{1,2}\/)/i.test(value)
}

async function readBuildAsset(outDir, assetPath, consumedAssets) {
  const normalized = assetPath.replace(/^\//, "")
  assertInside(outDir, join(outDir, normalized))
  consumedAssets?.add(normalized)
  return readFile(join(outDir, normalized), "utf8")
}

async function replaceAsync(source, pattern, replacer) {
  const matches = Array.from(source.matchAll(pattern))
  let result = ""
  let lastIndex = 0

  for (const match of matches) {
    result += source.slice(lastIndex, match.index)
    result += await replacer(...match)
    lastIndex = match.index + match[0].length
  }

  return result + source.slice(lastIndex)
}

function escapeInlineScript(source) {
  return source
    .replaceAll("</script", "<\\/script")
    .replaceAll("</SCRIPT", "<\\/SCRIPT")
}

function escapeInlineStyle(source) {
  return source
    .replaceAll("</style", "<\\/style")
    .replaceAll("</STYLE", "<\\/STYLE")
}

function pluginRootDir() {
  return resolve(fileURLToPath(new URL("../..", import.meta.url)))
}

function localStudioDir() {
  return join(pluginRootDir(), "apps", "local-studio")
}

function viteCliPath() {
  return join(pluginRootDir(), "node_modules", "vite", "bin", "vite.js")
}

function staticBuildDir() {
  const pluginManifest = JSON.parse(
    readFileSync(join(pluginRootDir(), ".codex-plugin", "plugin.json"), "utf8")
  )
  return join(tmpdir(), `myopenpanels-local-studio-v${pluginManifest.version}`)
}

function assertInside(parentDir, childPath) {
  const rel = relative(parentDir, childPath)
  if (rel.startsWith("..") || rel.includes(`..${sep}`)) {
    throw new Error(`Path escapes ${parentDir}: ${childPath}`)
  }
}
