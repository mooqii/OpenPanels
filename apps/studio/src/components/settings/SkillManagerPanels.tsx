import {
  Button,
  Checkbox,
  Chip,
  Dropdown,
  Label,
  ListBox,
  Modal,
  Select,
  Separator,
  Tooltip,
} from "@heroui/react"
import {
  Download,
  Eye,
  MoreHorizontal,
  Pencil,
  Plus,
  RefreshCw,
  Trash2,
  TriangleAlert,
  X,
} from "lucide-react"
import { useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import type {
  DeviceSkillGroup,
  ManagedProjectSkill,
  ManagedSkillModule,
  SkillUpdateState,
} from "../../types"

const ASSOCIATION_MODULES = [
  "wiki-update",
  "writing",
  "writing-refinement",
  "typesetting-cover",
  "publishing",
] as const

export function managedSkillActionIds(
  skill: ManagedProjectSkill,
  updateState?: SkillUpdateState
) {
  const actions = ["open"]
  if (updateState?.status === "updateAvailable") actions.push("update")
  if (skill.canDelete) actions.push("delete")
  return actions
}

export function InstalledSkillsPanel({
  isLoading,
  isCheckingUpdates,
  modules,
  onCheckUpdates,
  onDelete,
  onOpen,
  onUpdate,
  systemSkills,
  updateStates,
  updatingSkillId,
}: {
  isCheckingUpdates: boolean
  isLoading: boolean
  modules: ManagedSkillModule[]
  onCheckUpdates: () => void
  onDelete: (skill: ManagedProjectSkill) => void
  onOpen: (skill: ManagedProjectSkill) => void
  onUpdate: (skill: ManagedProjectSkill) => void
  systemSkills: ManagedProjectSkill[]
  updateStates: Record<string, SkillUpdateState>
  updatingSkillId: string | null
}) {
  const { t } = useMyOpenPanelsI18n()
  if (isLoading)
    return <div className="op-skill-manager__empty">{t`Loading...`}</div>
  return (
    <>
      <div className="op-skill-manager__toolbar op-skill-manager__toolbar--installed">
        <Tooltip closeDelay={0} delay={300}>
          <Button
            aria-label={t`Check Skill updates`}
            isIconOnly
            isPending={isCheckingUpdates}
            onPress={onCheckUpdates}
            size="sm"
            variant="ghost"
          >
            <RefreshCw size={15} />
          </Button>
          <Tooltip.Content placement="bottom">
            {t`Check Skill updates`}
          </Tooltip.Content>
        </Tooltip>
      </div>
      <div className="op-skill-sections">
        <SkillSection
          isCheckingUpdates={isCheckingUpdates}
          onDelete={onDelete}
          onOpen={onOpen}
          onUpdate={onUpdate}
          skills={systemSkills}
          title={t`MyOpenPanels system`}
          updateStates={updateStates}
          updatingSkillId={updatingSkillId}
        />
        {modules.map((module) => (
          <SkillSection
            isCheckingUpdates={isCheckingUpdates}
            key={module.kind}
            onDelete={onDelete}
            onOpen={onOpen}
            onUpdate={onUpdate}
            skills={module.skills}
            title={moduleLabel(module.kind, t)}
            updateStates={updateStates}
            updatingSkillId={updatingSkillId}
          />
        ))}
      </div>
    </>
  )
}

function SkillSection({
  isCheckingUpdates,
  onDelete,
  onOpen,
  onUpdate,
  skills,
  title,
  updateStates,
  updatingSkillId,
}: {
  isCheckingUpdates: boolean
  onDelete: (skill: ManagedProjectSkill) => void
  onOpen: (skill: ManagedProjectSkill) => void
  onUpdate: (skill: ManagedProjectSkill) => void
  skills: ManagedProjectSkill[]
  title: string
  updateStates: Record<string, SkillUpdateState>
  updatingSkillId: string | null
}) {
  if (skills.length === 0) return null
  return (
    <section className="op-skill-section">
      <h3>{title}</h3>
      <div className="op-skill-list">
        {skills.map((skill) => (
          <SkillRow
            isCheckingUpdates={isCheckingUpdates}
            key={skill.id}
            onDelete={onDelete}
            onOpen={onOpen}
            onUpdate={onUpdate}
            skill={skill}
            updateState={updateStates[skill.id]}
            updatingSkillId={updatingSkillId}
          />
        ))}
      </div>
    </section>
  )
}

function SkillRow({
  isCheckingUpdates,
  onDelete,
  onOpen,
  onUpdate,
  skill,
  updateState,
  updatingSkillId,
}: {
  isCheckingUpdates: boolean
  onDelete: (skill: ManagedProjectSkill) => void
  onOpen: (skill: ManagedProjectSkill) => void
  onUpdate: (skill: ManagedProjectSkill) => void
  skill: ManagedProjectSkill
  updateState?: SkillUpdateState
  updatingSkillId: string | null
}) {
  const { t } = useMyOpenPanelsI18n()
  const actionIds = managedSkillActionIds(skill, updateState)
  const updatePresentation = skillUpdatePresentation(
    skill,
    updateState,
    isCheckingUpdates,
    t
  )
  return (
    <div className="op-skill-row">
      <div className="op-skill-row__content">
        <div className="op-skill-row__title">
          <strong>{skill.name}</strong>
          <Chip size="sm" variant="soft">
            {skill.kind === "system"
              ? t`System`
              : skill.kind === "preset"
                ? t`Preset`
                : t`Self-built`}
          </Chip>
          {updatePresentation ? (
            <Chip
              className={`op-skill-update-chip op-skill-update-chip--${updatePresentation.tone}`}
              size="sm"
              title={updateState?.message}
              variant="soft"
            >
              {updatePresentation.label}
            </Chip>
          ) : null}
        </div>
        <span>{skill.description}</span>
        {skill.kind === "custom" ? (
          <span
            className="op-skill-row__source"
            title={skill.provenance?.sourceLocator}
          >
            {skill.provenance
              ? `${skillSourceLabel(skill.provenance.sourceType, t)} · ${skill.provenance.sourceLocator}`
              : t`Local import`}
          </span>
        ) : null}
      </div>
      <Dropdown>
        <Button
          aria-label={`${t`Skill actions`}: ${skill.name}`}
          isIconOnly
          isPending={updatingSkillId === skill.id}
          size="sm"
          variant="ghost"
        >
          <MoreHorizontal size={16} />
        </Button>
        <Dropdown.Popover>
          <Dropdown.Menu
            onAction={(key) => {
              if (key === "open") onOpen(skill)
              if (key === "update") onUpdate(skill)
              if (key === "delete") onDelete(skill)
            }}
          >
            <Dropdown.Item
              id="open"
              textValue={skill.canEdit ? t`Edit` : t`View`}
            >
              {skill.canEdit ? <Pencil size={14} /> : <Eye size={14} />}
              <Label>{skill.canEdit ? t`Edit` : t`View`}</Label>
            </Dropdown.Item>
            {actionIds.includes("update") ? (
              <Dropdown.Item id="update" textValue={t`Update Skill`}>
                <Download size={14} />
                <Label>{t`Update Skill`}</Label>
              </Dropdown.Item>
            ) : null}
            {actionIds.includes("delete") ? <Separator /> : null}
            {actionIds.includes("delete") ? (
              <Dropdown.Item id="delete" textValue={t`Delete`} variant="danger">
                <Trash2 size={14} />
                <Label>{t`Delete`}</Label>
              </Dropdown.Item>
            ) : null}
          </Dropdown.Menu>
        </Dropdown.Popover>
      </Dropdown>
    </div>
  )
}

export function skillUpdatePresentation(
  skill: ManagedProjectSkill,
  state: SkillUpdateState | undefined,
  isChecking: boolean,
  t: (value: TemplateStringsArray) => string
): { label: string; tone: string } | null {
  if (skill.kind !== "custom") return null
  if (isChecking && skill.canCheckUpdates) {
    return { label: t`Checking`, tone: "checking" }
  }
  if (!state && skill.canCheckUpdates) {
    return { label: t`Not checked`, tone: "neutral" }
  }
  if (!state || state.status === "unmanaged") {
    return { label: t`Updates unavailable`, tone: "neutral" }
  }
  if (state.status === "sourceUnavailable") {
    return {
      label: state.localModified
        ? t`Source unavailable · Local changes`
        : t`Source unavailable`,
      tone: "danger",
    }
  }
  if (state.status === "updateAvailable") {
    return {
      label: state.localModified
        ? t`Update available · Local changes`
        : t`Update available`,
      tone: "warning",
    }
  }
  if (state.localModified) {
    return { label: t`Local changes`, tone: "warning" }
  }
  return { label: t`Up to date`, tone: "success" }
}

function skillSourceLabel(
  sourceType: string,
  t: (value: TemplateStringsArray) => string
) {
  if (sourceType === "github") return "GitHub"
  if (sourceType === "skills-sh") return "skills.sh"
  if (sourceType === "clawhub") return "ClawHub"
  if (sourceType === "skillhub") return "SkillHub"
  if (sourceType === "device") return t`Device`
  return sourceType
}

export function DeviceSkillsPanel({
  isLoading,
  locations,
  onAdd,
  onLocationChange,
  onMismatch,
  onRemove,
  skills,
}: {
  isLoading: boolean
  locations: Record<string, string>
  onAdd: (skill: DeviceSkillGroup, locationPath: string) => void
  onLocationChange: (skillKey: string, path: string) => void
  onMismatch: (skill: DeviceSkillGroup, locationPath: string) => void
  onRemove: (skill: DeviceSkillGroup, moduleKind: string) => void
  skills: DeviceSkillGroup[]
}) {
  const { t } = useMyOpenPanelsI18n()
  if (isLoading)
    return (
      <div className="op-skill-manager__empty">{t`Scanning device Skills`}</div>
    )
  if (skills.length === 0)
    return (
      <div className="op-skill-manager__empty">{t`No device Skills found`}</div>
    )
  return (
    <div className="op-device-skill-list">
      {skills.map((skill) => {
        const selectedPath =
          locations[skill.key] ?? skill.locations[0]?.path ?? ""
        const selected = skill.locations.find(
          (location) => location.path === selectedPath
        )
        return (
          <section className="op-device-skill" key={skill.key}>
            <div className="op-device-skill__header">
              <strong>{skill.name}</strong>
              <div className="op-device-skill__associations">
                {(skill.installed?.moduleKinds ?? []).map((moduleKind) => (
                  <Chip
                    className="op-device-skill__association"
                    key={moduleKind}
                    size="sm"
                    variant="soft"
                  >
                    <span>{moduleLabel(moduleKind, t)}</span>
                    {skill.installed?.canManageAssociations ? (
                      <button
                        aria-label={`${t`Remove association`}: ${moduleLabel(moduleKind, t)}`}
                        onClick={() => onRemove(skill, moduleKind)}
                        type="button"
                      >
                        <X size={12} />
                      </button>
                    ) : null}
                  </Chip>
                ))}
                {selected?.comparison === "different" &&
                skill.installed?.canManageAssociations ? (
                  <Button
                    aria-label={t`Skill content differs`}
                    isIconOnly
                    onPress={() => onMismatch(skill, selectedPath)}
                    size="sm"
                    variant="danger-soft"
                  >
                    <TriangleAlert size={15} />
                  </Button>
                ) : null}
                <Button
                  aria-label={`${t`Add association`}: ${skill.name}`}
                  isDisabled={
                    !selected ||
                    (skill.installed !== null &&
                      !skill.installed.canManageAssociations) ||
                    skill.installed?.moduleKinds.length ===
                      ASSOCIATION_MODULES.length
                  }
                  isIconOnly
                  onPress={() => onAdd(skill, selectedPath)}
                  size="sm"
                  variant="ghost"
                >
                  <Plus size={15} />
                </Button>
              </div>
            </div>
            <p className="op-device-skill__description">
              {selected?.description ?? skill.description}
            </p>
            {skill.locations.length > 1 ? (
              <Select
                aria-label={`${t`Skill directory`}: ${skill.name}`}
                className="op-device-skill__select"
                onChange={(key) => onLocationChange(skill.key, String(key))}
                selectionMode="single"
                value={selectedPath}
                variant="secondary"
              >
                <Select.Trigger>
                  <Select.Value />
                  <Select.Indicator />
                </Select.Trigger>
                <Select.Popover>
                  <ListBox>
                    {skill.locations.map((location) => (
                      <ListBox.Item
                        id={location.path}
                        key={location.path}
                        textValue={location.path}
                      >
                        {location.path}
                      </ListBox.Item>
                    ))}
                  </ListBox>
                </Select.Popover>
              </Select>
            ) : null}
            {selected ? (
              <div className="op-device-skill__location">
                <span>
                  {selected.scope === "project" ? t`Project` : t`Global`}
                </span>
                <span>{selected.agents.join(", ")}</span>
                <code>{selected.path}</code>
              </div>
            ) : null}
          </section>
        )
      })}
    </div>
  )
}

export function moduleLabel(
  kind: string,
  t: (value: TemplateStringsArray) => string
) {
  if (kind === "wiki-update") return t`Wiki updates`
  if (kind === "writing") return t`Writing`
  if (kind === "writing-refinement") return t`Writing refinement`
  if (kind === "typesetting-cover") return t`Cover creation`
  if (kind === "publishing" || kind === "publishing-xiaohongshu")
    return t`Content publishing`
  if (kind === "wiki") return t`Wiki`
  if (kind === "canvas") return t`Canvas`
  if (kind === "typesetting") return t`Typesetting`
  if (kind === "unassociated") return t`Unassociated Skills`
  return kind
}

export function AssociationDialog({
  associated,
  isBusy,
  onClose,
  onSave,
}: {
  associated: string[]
  isBusy: boolean
  onClose: () => void
  onSave: (moduleKinds: string[]) => void
}) {
  const { t } = useMyOpenPanelsI18n()
  const [selectedModules, setSelectedModules] = useState<string[]>([])
  return (
    <Modal.Backdrop
      className="op-skill-manager-child-backdrop"
      isOpen
      onOpenChange={(open) => !open && onClose()}
    >
      <Modal.Container>
        <Modal.Dialog className="op-skill-association-dialog">
          <Modal.CloseTrigger aria-label={t`Close`} />
          <Modal.Header>
            <Modal.Heading>{t`Add Skill association`}</Modal.Heading>
          </Modal.Header>
          <Modal.Body>
            <p className="op-skill-association-dialog__description">
              {t`After association, this Skill can be called from the selected modules. Make sure the Skill capability matches each module for reliable results.`}
            </p>
            <div className="op-skill-association-options">
              {ASSOCIATION_MODULES.map((moduleKind) => {
                const isAssociated = associated.includes(moduleKind)
                return (
                  <div className="op-skill-association-option" key={moduleKind}>
                    <Checkbox
                      aria-label={moduleLabel(moduleKind, t)}
                      id={`skill-association-${moduleKind}`}
                      isDisabled={isAssociated || isBusy}
                      isSelected={
                        isAssociated || selectedModules.includes(moduleKind)
                      }
                      onChange={(isSelected) =>
                        setSelectedModules((current) =>
                          isSelected
                            ? [...current, moduleKind]
                            : current.filter((value) => value !== moduleKind)
                        )
                      }
                      variant="secondary"
                    >
                      <Checkbox.Content>
                        <Checkbox.Control>
                          <Checkbox.Indicator />
                        </Checkbox.Control>
                      </Checkbox.Content>
                    </Checkbox>
                    <label htmlFor={`skill-association-${moduleKind}`}>
                      <span>{moduleLabel(moduleKind, t)}</span>
                      {isAssociated ? <small>{t`Associated`}</small> : null}
                    </label>
                  </div>
                )
              })}
            </div>
          </Modal.Body>
          <Modal.Footer>
            <Button isDisabled={isBusy} onPress={onClose} variant="ghost">
              {t`Cancel`}
            </Button>
            <Button
              isDisabled={selectedModules.length === 0}
              isPending={isBusy}
              onPress={() => onSave(selectedModules)}
              variant="primary"
            >
              {t`Save`}
            </Button>
          </Modal.Footer>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}

export function MismatchDialog({
  isBusy,
  onClose,
  onIgnore,
  onReplace,
  skillName,
}: {
  isBusy: boolean
  onClose: () => void
  onIgnore: () => void
  onReplace: () => void
  skillName: string
}) {
  const { t } = useMyOpenPanelsI18n()
  return (
    <Modal.Backdrop
      className="op-skill-manager-child-backdrop"
      isDismissable={!isBusy}
      isOpen
      onOpenChange={(open) => !open && onClose()}
    >
      <Modal.Container>
        <Modal.Dialog className="op-skill-association-dialog">
          <Modal.CloseTrigger aria-label={t`Close`} />
          <Modal.Header>
            <Modal.Heading>{t`Skill content differs`}</Modal.Heading>
          </Modal.Header>
          <Modal.Body>
            <p>
              {skillName}:{" "}
              {t`The selected device copy differs from the installed copy.`}
            </p>
          </Modal.Body>
          <Modal.Footer>
            <Button isDisabled={isBusy} onPress={onIgnore} variant="ghost">
              {t`Ignore until content changes`}
            </Button>
            <Button isPending={isBusy} onPress={onReplace} variant="primary">
              {t`Replace with device Skill`}
            </Button>
          </Modal.Footer>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}
