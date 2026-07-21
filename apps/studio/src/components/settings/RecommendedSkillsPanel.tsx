import { Button, Chip } from "@heroui/react"
import { Download, Link2, PackagePlus, PackageSearch } from "lucide-react"
import { useMyOpenPanelsI18n } from "../../canvas"
import type { RecommendedSkill, SkillUpdateState } from "../../types"
import { moduleLabel } from "./SkillManagerPanels"

const RECOMMENDED_MODULE_ORDER = [
  "wiki-update",
  "writing",
  "writing-refinement",
  "typesetting-cover",
  "publishing",
]

type RecommendedSkillAction = "install" | "associate" | "update"

export function groupRecommendedSkills(
  skills: RecommendedSkill[],
  initialModuleKind = ""
) {
  const moduleKinds = Array.from(
    new Set(skills.flatMap((skill) => skill.moduleKinds))
  ).sort((left, right) => {
    if (left === initialModuleKind) return -1
    if (right === initialModuleKind) return 1
    const leftIndex = RECOMMENDED_MODULE_ORDER.indexOf(left)
    const rightIndex = RECOMMENDED_MODULE_ORDER.indexOf(right)
    return (
      (leftIndex === -1 ? Number.MAX_SAFE_INTEGER : leftIndex) -
        (rightIndex === -1 ? Number.MAX_SAFE_INTEGER : rightIndex) ||
      left.localeCompare(right)
    )
  })
  return moduleKinds.map((kind) => ({
    kind,
    skills: skills.filter((skill) => skill.moduleKinds.includes(kind)),
  }))
}

export function recommendedSkillAction(
  skill: RecommendedSkill,
  updateState?: SkillUpdateState
): RecommendedSkillAction | null {
  if (skill.installStatus === "conflict") return null
  if (!skill.installedSkillId) return "install"
  if (updateState?.status === "updateAvailable") return "update"
  if (skill.installStatus === "bindingsMissing") return "associate"
  return null
}

export function recommendedSkillPresentation(
  skill: RecommendedSkill,
  state: SkillUpdateState | undefined,
  isChecking: boolean,
  isPending: boolean,
  t: (value: TemplateStringsArray) => string
) {
  if (isPending) return { label: t`Installing`, tone: "checking" }
  if (skill.installStatus === "conflict") {
    return { label: t`Skill conflict`, tone: "danger" }
  }
  if (!skill.installedSkillId) {
    return { label: t`Not installed`, tone: "neutral" }
  }
  if (isChecking && skill.canCheckUpdates) {
    return { label: t`Checking`, tone: "checking" }
  }
  if (state?.status === "sourceUnavailable") {
    return {
      label: state.localModified
        ? t`Source unavailable · Local changes`
        : t`Source unavailable`,
      tone: "danger",
    }
  }
  if (state?.status === "updateAvailable") {
    return {
      label: state.localModified
        ? t`Update available · Local changes`
        : t`Update available`,
      tone: "warning",
    }
  }
  if (skill.installStatus === "bindingsMissing") {
    return { label: t`Module association needed`, tone: "warning" }
  }
  if (state?.localModified) {
    return { label: t`Local changes`, tone: "warning" }
  }
  if (state?.status === "upToDate") {
    return { label: t`Up to date`, tone: "success" }
  }
  return { label: t`Installed`, tone: "success" }
}

export function RecommendedSkillsPanel({
  initialModuleKind,
  isCheckingUpdates,
  isLoading,
  onInstall,
  onUpdate,
  pendingCatalogId,
  skills,
  updateStates,
  updatingSkillId,
}: {
  initialModuleKind?: string
  isCheckingUpdates: boolean
  isLoading: boolean
  onInstall: (skill: RecommendedSkill) => void
  onUpdate: (skill: RecommendedSkill) => void
  pendingCatalogId: string | null
  skills: RecommendedSkill[]
  updateStates: Record<string, SkillUpdateState>
  updatingSkillId: string | null
}) {
  const { t } = useMyOpenPanelsI18n()
  if (isLoading) {
    return <div className="op-skill-manager__empty">{t`Loading...`}</div>
  }
  if (skills.length === 0) {
    return (
      <div className="op-recommended-skills__empty">
        <PackageSearch aria-hidden size={24} />
        <strong>{t`No recommended Skills yet`}</strong>
        <span>{t`Recommended Skills will appear here in a future app update.`}</span>
      </div>
    )
  }

  return (
    <div className="op-recommended-skills">
      {groupRecommendedSkills(skills, initialModuleKind).map((group) => (
        <section className="op-skill-section" key={group.kind}>
          <h3>{moduleLabel(group.kind, t)}</h3>
          <div className="op-recommended-skill-list">
            {group.skills.map((skill) => {
              const updateState = skill.installedSkillId
                ? updateStates[skill.installedSkillId]
                : undefined
              const isInstalling = pendingCatalogId === skill.id
              const isUpdating =
                skill.installedSkillId === updatingSkillId &&
                updatingSkillId !== null
              const action = recommendedSkillAction(skill, updateState)
              const presentation = recommendedSkillPresentation(
                skill,
                updateState,
                isCheckingUpdates,
                isInstalling || isUpdating,
                t
              )
              return (
                <div className="op-recommended-skill-row" key={skill.id}>
                  <div className="op-recommended-skill-row__content">
                    <div className="op-recommended-skill-row__title">
                      <strong>{skill.name}</strong>
                      <Chip
                        className={`op-skill-update-chip op-skill-update-chip--${presentation.tone}`}
                        size="sm"
                        title={updateState?.message}
                        variant="soft"
                      >
                        {presentation.label}
                      </Chip>
                    </div>
                    <span>{skill.description}</span>
                    <div className="op-recommended-skill-row__metadata">
                      <span title={skill.sourceLocator}>
                        {`${
                          skill.sourceType === "github"
                            ? "GitHub"
                            : skill.sourceType === "skills-sh"
                              ? "skills.sh"
                              : skill.sourceType === "clawhub"
                                ? "ClawHub"
                                : "SkillHub"
                        } · ${skill.sourceLocator}`}
                      </span>
                      <div>
                        {skill.moduleKinds.map((moduleKind) => (
                          <Chip key={moduleKind} size="sm" variant="soft">
                            {moduleLabel(moduleKind, t)}
                          </Chip>
                        ))}
                      </div>
                    </div>
                  </div>
                  {action ? (
                    <Button
                      isDisabled={isInstalling || isUpdating}
                      isPending={isInstalling || isUpdating}
                      onPress={() =>
                        action === "update" ? onUpdate(skill) : onInstall(skill)
                      }
                      size="sm"
                      variant={action === "update" ? "primary" : "secondary"}
                    >
                      {action === "update" ? (
                        <Download size={14} />
                      ) : action === "associate" ? (
                        <Link2 size={14} />
                      ) : (
                        <PackagePlus size={14} />
                      )}
                      {action === "update"
                        ? t`Update Skill`
                        : action === "associate"
                          ? t`Associate modules`
                          : t`Install`}
                    </Button>
                  ) : null}
                </div>
              )
            })}
          </div>
        </section>
      ))}
    </div>
  )
}
