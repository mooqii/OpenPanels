import { existsSync, readdirSync, readFileSync, statSync } from "node:fs"
import {
  renderCapabilityMatrix,
  renderEntryCapabilityIndex,
} from "./lib/capability-projections.mjs"

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
const capabilityDoc = readFileSync(
  new URL("docs/module-capabilities.md", ROOT),
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
const currentContractSources = [
  "crates/myopenpanels/src/agent/bootstrap.rs",
  "crates/myopenpanels/src/agent/skills_context.rs",
  "crates/myopenpanels/src/agent/skill_parsing.rs",
  "crates/myopenpanels/src/agent/skill_management.rs",
  "crates/myopenpanels/src/control/runtime.rs",
  "crates/myopenpanels/src/wiki/markdown_skills.rs",
  "crates/myopenpanels/src/writing/requests.rs",
].map((path) => readFileSync(new URL(path, ROOT), "utf8"))
const retiredRuntimeContracts = [
  "karpathy-llm-wiki",
  "myopenpanels-canvas-panel",
  "myopenpanels-wiki-panel",
  "myopenpanels-writing-panel",
  "writing-skill-distiller",
  "custom_writing_skill_from_source",
  "custom_writing_skills_dir",
  "legacy_skill_from_parts",
  "migrate_legacy_custom_agent_skills",
]
for (const retired of retiredRuntimeContracts) {
  assert(
    currentContractSources.every((source) => !source.includes(retired)),
    `Retired runtime contract must not return: ${retired}`
  )
}
const cliSynchronizedContractSources = [
  "agent-resources/builtin-skill-registry.json",
  "agent-resources/module-capability-catalog.json",
  "agent-resources/recommended-skills.json",
  "crates/myopenpanels/src/capabilities.rs",
  "crates/myopenpanels/src/agent/bootstrap.rs",
  "crates/myopenpanels/src/agent/procedures.rs",
  "crates/myopenpanels/src/agent/recommended_skills.rs",
  "crates/myopenpanels/src/agent/skill_associations.rs",
  "crates/myopenpanels/src/agent/skill_import.rs",
  "crates/myopenpanels/src/agent/skill_parsing.rs",
  "crates/myopenpanels/src/agent/skill_updates.rs",
  "crates/myopenpanels/src/bridge/document_prompts.rs",
  "crates/myopenpanels/src/bridge/finalization.rs",
  "crates/myopenpanels/src/bridge/result_validation.rs",
  "crates/myopenpanels/src/bridge/task_handlers.rs",
  "crates/myopenpanels/src/bridge/publication_prompts.rs",
  "crates/myopenpanels/src/bridge/writing_wiki_prompts.rs",
  "crates/myopenpanels/src/cli/runtime_core.rs",
  "crates/myopenpanels/src/cli/registry/specs.rs",
  "crates/myopenpanels/src/cli/registry/types.rs",
  "crates/myopenpanels/src/content/filesystem.rs",
  "crates/myopenpanels/src/control/runtime.rs",
  "crates/myopenpanels/src/operations/runtime.rs",
  "crates/myopenpanels/src/release.rs",
  "crates/myopenpanels/src/tasks/handoffs.rs",
  "crates/myopenpanels/src/tasks/query_targets.rs",
  "crates/myopenpanels/src/publication.rs",
  "crates/myopenpanels/src/wiki/materialization.rs",
  "crates/myopenpanels/src/wiki/state.rs",
  "crates/myopenpanels/src/writing/requests.rs",
  "crates/myopenpanels/src/writing/skills.rs",
  "apps/studio/src/canvas/store.ts",
  "apps/studio/src/canvas/types/records.ts",
  "apps/studio/src/lib/api.ts",
  "apps/studio/src/lib/typesetting.ts",
  "apps/studio/src/types.ts",
].map((path) => ({ path, source: readFileSync(new URL(path, ROOT), "utf8") }))
for (const forbidden of [
  '"schemaVersion"',
  "schema_version",
  '"protocolVersion"',
  "protocol_version",
  '"catalogVersion"',
  "CATALOG_VERSION",
  "PROTOCOL_VERSION",
  '"adapterVersion"',
  "adapter_version",
  '"taskHandoffVersion"',
]) {
  for (const { path, source } of cliSynchronizedContractSources) {
    assert(
      !source.includes(forbidden),
      `CLI-synchronized contract must not carry an independent version (${forbidden}): ${path}`
    )
  }
}
assert(
  !cliSource.includes('"  agent context'),
  "Retired agent context must not return to the public CLI surface."
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
const capabilityCatalog = readJson(
  "agent-resources/module-capability-catalog.json"
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
const systemSkills = new Map()
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
      "Built-in Skill must use only name and description frontmatter: " +
        registration.id
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
            "Preset Skill " +
              registration.id +
              " contains platform contract text " +
              forbidden +
              ": " +
              file.pathname
          )
        }
      }
    } else {
      for (const legacyField of ["workflows", "procedures", "taskHandoffs"]) {
        assert(
          !(legacyField in registration),
          "System Skill registration must not own " +
            legacyField +
            ": " +
            registration.id
        )
      }
      systemSkills.set(registration.id, { packagePath, registration })
    }
  }
}

const agentRouteKeys = new Set()
const taskRoutes = new Set()
let procedureCount = 0
let taskCapabilityCount = 0
const validateCapabilityBase = (capability) => {
  assert(
    typeof capability.key === "string" && capability.key.length > 0,
    "Capability key is missing."
  )
  assert(
    typeof capability.description === "string" &&
      capability.description.length > 0,
    `Capability description is missing: ${capability.key}`
  )
  assert(
    !agentRouteKeys.has(capability.key),
    `Duplicate Agent route key: ${capability.key}`
  )
  agentRouteKeys.add(capability.key)
  const contract = capability.platformContract
  assert(
    contract && typeof contract === "object",
    `Capability platform contract is missing: ${capability.key}`
  )
  const skill = systemSkills.get(contract.systemSkillId)
  assert(
    skill,
    `Capability ${capability.key} references unknown system Skill ${contract.systemSkillId}.`
  )
  assert(
    capability.panelKind === null ||
      skill.registration.appliesTo.includes("any") ||
      skill.registration.appliesTo.includes(capability.panelKind),
    `Capability panel kind does not match its system Skill: ${capability.key}`
  )
  assert(
    Array.isArray(contract.references) && contract.references.length > 0,
    `Capability platform references are missing: ${capability.key}`
  )
  assert(
    new Set(contract.references).size === contract.references.length,
    `Capability platform references are duplicated: ${capability.key}`
  )
  for (const reference of contract.references) {
    assert(
      typeof reference === "string" &&
        reference.length > 0 &&
        !reference.startsWith("/") &&
        !reference.split("/").includes(".."),
      `Capability platform reference is invalid: ${capability.key}`
    )
    assert(
      existsSync(new URL(`${skill.packagePath}/${reference}`, ROOT)),
      `Capability platform reference is missing: ${capability.key}`
    )
  }
  const localSkill = capability.localSkill
  assert(
    localSkill &&
      ["none", "optional", "required", "fixed"].includes(localSkill.mode),
    `Capability Local Skill policy is invalid: ${capability.key}`
  )
  assert(
    localSkill.mode === "fixed"
      ? typeof localSkill.skillId === "string" && localSkill.skillId.length > 0
      : !("skillId" in localSkill),
    `Capability fixed Local Skill policy is invalid: ${capability.key}`
  )
  assert(
    entrySkill.includes(`\`${capability.key}\``),
    `Entry Skill is missing Capability route: ${capability.key}`
  )
}

assert(
  Array.isArray(capabilityCatalog.capabilities),
  "Module Capability Catalog capabilities must be an array."
)
for (const capability of capabilityCatalog.capabilities) {
  validateCapabilityBase(capability)
  const invocation = capability.invocation
  assert(
    invocation && typeof invocation === "object",
    `Capability invocation is missing: ${capability.key}`
  )
  if (invocation.kind === "procedure") {
    procedureCount += 1
    assert(
      !("taskPointer" in capability.localSkill),
      `Procedure Local Skill policy cannot declare taskPointer: ${capability.key}`
    )
    assert(
      [
        "none",
        "summary",
        "optional-detail",
        "active-detail",
        "explicit-detail",
      ].includes(invocation.selectionPolicy),
      `Invalid Agent Procedure selectionPolicy: ${capability.key}`
    )
    assert(
      Array.isArray(invocation.commandIntents) &&
        invocation.commandIntents.length > 0,
      `Agent Procedure command intents are missing: ${capability.key}`
    )
    continue
  }
  taskCapabilityCount += 1
  if (invocation.kind === "task") {
    assert(
      capability.localSkill.mode === "none"
        ? !("taskPointer" in capability.localSkill)
        : typeof capability.localSkill.taskPointer === "string" &&
            /^\/(input|source)\//.test(capability.localSkill.taskPointer),
      `Task Local Skill pointer is invalid: ${capability.key}`
    )
    assert(
      Array.isArray(invocation.routes) && invocation.routes.length > 0,
      `Task Capability routes are missing: ${capability.key}`
    )
    for (const route of invocation.routes) {
      const routeKey = [route.queue, route.taskType, route.capability].join(
        "\u0000"
      )
      assert(
        !taskRoutes.has(routeKey),
        `Duplicate Task Capability route: ${capability.key}`
      )
      taskRoutes.add(routeKey)
      for (const field of ["queue", "taskType", "capability", "handlerKey"]) {
        assert(
          typeof route[field] === "string" && route[field].length > 0,
          `Task Capability route field is missing: ${capability.key}.${field}`
        )
      }
    }
    continue
  }
  assert(
    invocation.kind === "task-scope" &&
      capability.localSkill.mode === "none" &&
      !("taskPointer" in capability.localSkill) &&
      Array.isArray(invocation.scopeKinds) &&
      invocation.scopeKinds.length > 0 &&
      new Set(invocation.scopeKinds).size === invocation.scopeKinds.length &&
      invocation.scopeKinds.every((scopeKind) =>
        ["exact-task", "project-drain", "wiki-mutation-drain"].includes(
          scopeKind
        )
      ),
    `Task Capability scope kinds are invalid: ${capability.key}`
  )
}

const expectedCapabilityIndex = renderEntryCapabilityIndex(capabilityCatalog)
const capabilityIndex = entrySkill.match(
  /<!-- BEGIN GENERATED CAPABILITY INDEX -->\n([\s\S]*?)\n<!-- END GENERATED CAPABILITY INDEX -->/
)?.[1]
assert(
  capabilityIndex === expectedCapabilityIndex,
  "MyOpenPanels Entry Skill Capability index is out of date."
)
const expectedCapabilityMatrix = renderCapabilityMatrix(capabilityCatalog)
const capabilityMatrix = capabilityDoc.match(
  /<!-- BEGIN GENERATED CAPABILITY MATRIX -->\n([\s\S]*?)\n<!-- END GENERATED CAPABILITY MATRIX -->/
)?.[1]
assert(
  capabilityMatrix === expectedCapabilityMatrix,
  "Panel Capability matrix is out of date."
)

assert(
  procedureCount === 19,
  `Expected 19 Agent Procedures; got ${procedureCount}.`
)
assert(
  taskCapabilityCount === 9,
  `Expected 9 Task Capabilities; got ${taskCapabilityCount}.`
)
assert(
  taskRoutes.size === 10,
  `Expected 10 Task routes; got ${taskRoutes.size}.`
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
