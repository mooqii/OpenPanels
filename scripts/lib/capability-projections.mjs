export const CAPABILITY_INDEX_START =
  "<!-- BEGIN GENERATED CAPABILITY INDEX -->"
export const CAPABILITY_INDEX_END = "<!-- END GENERATED CAPABILITY INDEX -->"
export const CAPABILITY_MATRIX_START =
  "<!-- BEGIN GENERATED CAPABILITY MATRIX -->"
export const CAPABILITY_MATRIX_END = "<!-- END GENERATED CAPABILITY MATRIX -->"

export function renderEntryCapabilityIndex(catalog) {
  const panelLabels = new Map([
    ["canvas", "Canvas"],
    ["wiki", "Wiki"],
    ["writing", "Writing"],
    ["typesetting", "Typesetting"],
    ["publishing", "Publishing"],
    [null, "Task queue"],
  ])
  const lines = []
  for (const [panelKind, label] of panelLabels) {
    const procedures = catalog.capabilities.filter(
      (capability) =>
        capability.panelKind === panelKind &&
        capability.invocation.kind === "procedure"
    )
    if (procedures.length === 0) {
      continue
    }
    lines.push(`${label}:`, "")
    for (const procedure of procedures) {
      lines.push(`- \`${procedure.key}\`: ${procedure.description}`)
    }
    lines.push("")
  }
  lines.push("Task Handoffs must never be passed to Procedure Bootstrap:", "")
  for (const capability of catalog.capabilities.filter(
    (candidate) => candidate.invocation.kind !== "procedure"
  )) {
    lines.push(`- \`${capability.key}\`: ${capability.description}`)
  }
  lines.push("", "Execute their exact Studio or claimed Task handoff instead.")
  return lines.join("\n")
}

export function replaceEntryCapabilityIndex(source, rendered) {
  return replaceGeneratedSection(
    source,
    CAPABILITY_INDEX_START,
    CAPABILITY_INDEX_END,
    rendered
  )
}

export function renderCapabilityMatrix(catalog) {
  const surfaces = [
    ["canvas", "Canvas"],
    ["wiki", "Wiki"],
    ["writing", "Writing"],
    ["typesetting", "Typesetting"],
    ["publishing", "Publishing"],
    [null, "Generic Task queue"],
  ]
  const lines = [
    "| Surface | Direct Procedures | Task Capabilities | Task Routes |",
    "| --- | ---: | ---: | ---: |",
  ]
  let directTotal = 0
  let taskTotal = 0
  let routeTotal = 0
  for (const [panelKind, label] of surfaces) {
    const capabilities = catalog.capabilities.filter(
      (capability) => capability.panelKind === panelKind
    )
    const direct = capabilities.filter(
      (capability) => capability.invocation.kind === "procedure"
    ).length
    const tasks = capabilities.length - direct
    const routes = capabilities.reduce(
      (total, capability) =>
        total +
        (capability.invocation.kind === "task"
          ? capability.invocation.routes.length
          : 0),
      0
    )
    directTotal += direct
    taskTotal += tasks
    routeTotal += routes
    lines.push(`| ${label} | ${direct} | ${tasks} | ${routes} |`)
  }
  lines.push(`| Total | ${directTotal} | ${taskTotal} | ${routeTotal} |`)
  return lines.join("\n")
}

export function replaceCapabilityMatrix(source, rendered) {
  return replaceGeneratedSection(
    source,
    CAPABILITY_MATRIX_START,
    CAPABILITY_MATRIX_END,
    rendered
  )
}

function replaceGeneratedSection(source, start, end, rendered) {
  const expression = new RegExp(`${start}\\n[\\s\\S]*?\\n${end}`)
  if (!expression.test(source)) {
    throw new Error(`Generated projection markers are missing: ${start}`)
  }
  return source.replace(expression, `${start}\n${rendered}\n${end}`)
}
