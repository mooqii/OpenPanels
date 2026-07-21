import { existsSync, readdirSync, readFileSync, statSync } from "node:fs"

const ROOT = new URL("..", import.meta.url)
const RELEASE_TARGETS = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "x86_64-pc-windows-msvc",
]

function readJson(path) {
  return JSON.parse(readFileSync(new URL(path, ROOT), "utf8"))
}

function readCargoVersion(path) {
  const toml = readFileSync(new URL(path, ROOT), "utf8")
  const match = toml.match(/^version\s*=\s*"([^"]+)"/m)
  if (!match) throw new Error(`Missing package version in ${path}`)
  return match[1]
}

function assert(condition, message) {
  if (!condition) throw new Error(message)
}

function walkFiles(path) {
  const root = new URL(path, ROOT)
  return readdirSync(root, { withFileTypes: true }).flatMap((entry) => {
    const child = new URL(entry.name, `${root.href}/`)
    if (entry.isDirectory()) {
      return walkFiles(`${path}/${entry.name}`)
    }
    return statSync(child).isFile() ? [child] : []
  })
}

const rootVersion = readJson("package.json").version
const studioVersion = readJson("apps/studio/package.json").version
const rustVersion = readCargoVersion("crates/myopenpanels/Cargo.toml")
const tag =
  process.env.GITHUB_REF_NAME || process.env.RELEASE_TAG || `v${rootVersion}`
const tagVersion = tag.startsWith("v") ? tag.slice(1) : tag

assert(
  rootVersion === rustVersion,
  `Root package version ${rootVersion} does not match Rust CLI version ${rustVersion}.`
)
assert(
  rootVersion === studioVersion,
  `Root package version ${rootVersion} does not match Studio version ${studioVersion}.`
)

const entrySkill = readFileSync(
  new URL("skills/myopenpanels/SKILL.md", ROOT),
  "utf8"
)
const entrySkillInstall = readFileSync(
  new URL("skills/myopenpanels/references/install.md", ROOT),
  "utf8"
)
const entrySkillVersion = entrySkill.match(
  /^\s+version:\s*["']([^"']+)["']/m
)?.[1]
const entrySkillSource = entrySkill.match(
  /^\s+source:\s*["']([^"']+)["']/m
)?.[1]
const cliSource = readFileSync(
  new URL("crates/myopenpanels/src/cli.rs", ROOT),
  "utf8"
)
assert(
  !cliSource.includes('"  agent context'),
  "Protocol v1 agent context must not return to the public CLI surface."
)
assert(
  entrySkillVersion,
  "MyOpenPanels entry skill must declare metadata.version."
)
assert(
  entrySkillSource,
  "MyOpenPanels entry skill must declare metadata.source."
)
assert(
  entrySkillSource ===
    "https://github.com/mooqii/OpenPanels/tree/main/skills/myopenpanels",
  `MyOpenPanels entry skill source must target the canonical latest package; got ${entrySkillSource}.`
)
for (const required of [
  "agent bootstrap",
  "drawing",
  "organizing",
  "writing",
  "open or launch MyOpenPanels",
  "打开面板",
  "--procedure",
  "Task Handoff",
  "Workflow Runs",
]) {
  assert(
    entrySkill.includes(required),
    `MyOpenPanels entry skill must retain ${required}.`
  )
}
for (const required of [
  "install-myopenpanels.sh",
  "install-myopenpanels.ps1",
  "MYOPENPANELS_INSTALL_DIR",
]) {
  assert(
    entrySkillInstall.includes(required),
    `MyOpenPanels install reference must retain ${required}.`
  )
}
for (const forbidden of [
  "myopenpanels canvas ",
  "myopenpanels wiki ",
  "myopenpanels task ",
  "myopenpanels operation ",
  "--context-id",
  "--protocol-version",
  "minCliVersion",
  "Do not use package-manager",
  "Node-based fallback",
  "--workflow",
]) {
  assert(
    !entrySkill.includes(forbidden),
    `MyOpenPanels entry skill must not embed Panel Procedure detail: ${forbidden}.`
  )
}
assert(
  tag === `v${rootVersion}`,
  `Release tag must be v${rootVersion}; got ${tag}.`
)

const builtinRegistry = readJson("agent-resources/builtin-skill-registry.json")
assert(
  builtinRegistry.schemaVersion === 4,
  "Built-in Skill registry must use schemaVersion 4."
)
const forbiddenPortableSkillText = [
  "myopenpanels",
  "my open panels",
  "--task-id",
  "agent bootstrap",
  "agent skill read",
  "writing skill install",
  "operation complete",
  "task.claim",
  "task.heartbeat",
  "task.complete",
  "task.fail",
  "bridge-managed",
]
const builtinSkillIds = new Set()
const agentRouteKeys = new Set()
let agentProcedureCount = 0
let taskHandoffCount = 0
for (const [group, registrations] of [
  ["system-skills", builtinRegistry.systemSkills],
  ["preset-skills", builtinRegistry.presetSkills],
]) {
  assert(Array.isArray(registrations), `Missing built-in Skill group: ${group}`)
  const packageDirs = readdirSync(new URL(`agent-resources/${group}/`, ROOT), {
    withFileTypes: true,
  })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort()
  const registeredDirs = registrations
    .map((registration) => registration.packageDir)
    .sort()
  assert(
    JSON.stringify(packageDirs) === JSON.stringify(registeredDirs),
    `Built-in Skill packages and registrations differ in ${group}.`
  )
  for (const registration of registrations) {
    assert(
      registration.packageDir === registration.id,
      `Built-in Skill package directory must match its id: ${registration.id}`
    )
    assert(
      !builtinSkillIds.has(registration.id),
      `Duplicate built-in Skill id: ${registration.id}`
    )
    builtinSkillIds.add(registration.id)
    const packagePath = `agent-resources/${group}/${registration.packageDir}`
    const skill = readFileSync(new URL(`${packagePath}/SKILL.md`, ROOT), "utf8")
    const frontmatter = skill.match(/^---\n([\s\S]*?)\n---/)?.[1] ?? ""
    const keys = frontmatter
      .split("\n")
      .filter((line) => /^[A-Za-z][A-Za-z0-9-]*:/.test(line))
      .map((line) => line.slice(0, line.indexOf(":")))
      .sort()
    assert(
      JSON.stringify(keys) === JSON.stringify(["description", "name"]),
      `Built-in Skill must use only name and description frontmatter: ${registration.id}`
    )
    const skillName = frontmatter.match(/^name:\s*(.+)$/m)?.[1]?.trim()
    assert(
      skillName === registration.id,
      `Built-in Skill name must match its registered id: ${registration.id}`
    )
    if (group === "preset-skills") {
      for (const file of walkFiles(packagePath)) {
        const content = readFileSync(file, "utf8").toLowerCase()
        for (const forbidden of forbiddenPortableSkillText) {
          assert(
            !content.includes(forbidden),
            `Preset Skill ${registration.id} contains platform contract text ${forbidden}: ${file.pathname}`
          )
        }
      }
    } else {
      assert(
        !("workflows" in registration),
        `System Skill must not declare legacy Workflows: ${registration.id}`
      )
      assert(
        Array.isArray(registration.procedures),
        `System Skill must declare Procedures: ${registration.id}`
      )
      assert(
        Array.isArray(registration.taskHandoffs),
        `System Skill must declare Task Handoffs: ${registration.id}`
      )
      for (const procedure of registration.procedures) {
        assert(
          !agentRouteKeys.has(procedure.key),
          `Duplicate Agent Procedure key: ${procedure.key}`
        )
        agentRouteKeys.add(procedure.key)
        agentProcedureCount += 1
        assert(
          !("executionMode" in procedure),
          `Agent Procedure must not declare executionMode: ${procedure.key}`
        )
        assert(
          [
            "none",
            "summary",
            "optional-detail",
            "active-detail",
            "explicit-detail",
          ].includes(procedure.selectionPolicy),
          `Invalid Agent Procedure selectionPolicy: ${procedure.key}`
        )
        assert(
          Array.isArray(procedure.commandIntents) &&
            procedure.commandIntents.length > 0,
          `Agent Procedure command intents are missing: ${procedure.key}`
        )
        assert(
          Array.isArray(procedure.references) &&
            procedure.references.length > 0,
          `Agent Procedure references are missing: ${procedure.key}`
        )
        assert(
          new Set(procedure.references).size === procedure.references.length,
          `Agent Procedure references are duplicated: ${procedure.key}`
        )
        for (const reference of procedure.references) {
          assert(
            typeof reference === "string" &&
              reference.length > 0 &&
              !reference.startsWith("/") &&
              !reference.split("/").includes(".."),
            `Agent Procedure reference is invalid: ${procedure.key}`
          )
          assert(
            existsSync(new URL(`${packagePath}/${reference}`, ROOT)),
            `Agent Procedure reference is missing: ${procedure.key}`
          )
        }
        assert(
          entrySkill.includes(`\`${procedure.key}\``),
          `Entry Skill is missing Agent Procedure route: ${procedure.key}`
        )
      }
      for (const handoff of registration.taskHandoffs) {
        assert(
          !agentRouteKeys.has(handoff.key),
          `Duplicate Agent route key: ${handoff.key}`
        )
        agentRouteKeys.add(handoff.key)
        taskHandoffCount += 1
        assert(
          !("executionMode" in handoff || "selectionPolicy" in handoff),
          `Task Handoff must not declare Procedure fields: ${handoff.key}`
        )
        assert(
          Array.isArray(handoff.commandIntents) &&
            handoff.commandIntents.length > 0,
          `Task Handoff command intents are missing: ${handoff.key}`
        )
        assert(
          existsSync(new URL(`${packagePath}/${handoff.reference}`, ROOT)),
          `Task Handoff reference is missing: ${handoff.key}`
        )
        assert(
          entrySkill.includes(`\`${handoff.key}\``),
          `Entry Skill is missing Task Handoff route: ${handoff.key}`
        )
      }
    }
  }
}
assert(
  agentProcedureCount === 18,
  `Expected 18 Agent Procedures; got ${agentProcedureCount}.`
)
assert(
  taskHandoffCount === 5,
  `Expected 5 Task Handoffs; got ${taskHandoffCount}.`
)

const manifest = {
  schemaVersion: 1,
  name: "myopenpanels",
  version: tagVersion,
  channel: tagVersion.includes("-") ? "prerelease" : "stable",
  entrySkill: {
    id: "myopenpanels",
    version: entrySkillVersion,
    source: entrySkillSource,
  },
  assets: Object.fromEntries(
    RELEASE_TARGETS.map((target) => {
      const extension = target.includes("windows") ? "zip" : "tar.gz"
      const fileName = `myopenpanels-${target}.${extension}`
      return [
        target,
        {
          fileName,
          url: `https://github.com/mooqii/OpenPanels/releases/download/${tag}/${fileName}`,
          sha256: "<filled-by-release-workflow>",
          size: 0,
        },
      ]
    })
  ),
}

console.log(`Release constraints passed for MyOpenPanels ${tag}.`)
console.log(JSON.stringify(manifest, null, 2))
