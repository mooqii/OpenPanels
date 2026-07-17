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
} from "@heroui/react"
import {
  Eye,
  MoreHorizontal,
  Pencil,
  Plus,
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
} from "../../types"

const ASSOCIATION_MODULES = [
  "wiki-update",
  "writing",
  "writing-refinement",
] as const

export function managedSkillActionIds(skill: ManagedProjectSkill) {
  return skill.canDelete ? ["open", "delete"] : ["open"]
}

export function InstalledSkillsPanel({
  isLoading,
  modules,
  onDelete,
  onOpen,
  systemSkills,
}: {
  isLoading: boolean
  modules: ManagedSkillModule[]
  onDelete: (skill: ManagedProjectSkill) => void
  onOpen: (skill: ManagedProjectSkill) => void
  systemSkills: ManagedProjectSkill[]
}) {
  const { t } = useMyOpenPanelsI18n()
  if (isLoading)
    return <div className="op-skill-manager__empty">{t`Loading...`}</div>
  return (
    <div className="op-skill-sections">
      <SkillSection
        onDelete={onDelete}
        onOpen={onOpen}
        skills={systemSkills}
        title={t`MyOpenPanels system`}
      />
      {modules.map((module) => (
        <SkillSection
          key={module.kind}
          onDelete={onDelete}
          onOpen={onOpen}
          skills={module.skills}
          title={moduleLabel(module.kind, t)}
        />
      ))}
    </div>
  )
}

function SkillSection({
  onDelete,
  onOpen,
  skills,
  title,
}: {
  onDelete: (skill: ManagedProjectSkill) => void
  onOpen: (skill: ManagedProjectSkill) => void
  skills: ManagedProjectSkill[]
  title: string
}) {
  if (skills.length === 0) return null
  return (
    <section className="op-skill-section">
      <h3>{title}</h3>
      <div className="op-skill-list">
        {skills.map((skill) => (
          <SkillRow
            key={skill.id}
            onDelete={onDelete}
            onOpen={onOpen}
            skill={skill}
          />
        ))}
      </div>
    </section>
  )
}

function SkillRow({
  onDelete,
  onOpen,
  skill,
}: {
  onDelete: (skill: ManagedProjectSkill) => void
  onOpen: (skill: ManagedProjectSkill) => void
  skill: ManagedProjectSkill
}) {
  const { t } = useMyOpenPanelsI18n()
  const actionIds = managedSkillActionIds(skill)
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
        </div>
        <span>{skill.description}</span>
      </div>
      <Dropdown>
        <Button
          aria-label={`${t`Skill actions`}: ${skill.name}`}
          isIconOnly
          size="sm"
          variant="ghost"
        >
          <MoreHorizontal size={16} />
        </Button>
        <Dropdown.Popover>
          <Dropdown.Menu
            onAction={(key) => {
              if (key === "open") onOpen(skill)
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
  if (kind === "wiki") return t`Wiki`
  if (kind === "canvas") return t`Canvas`
  if (kind === "typesetting") return t`Typesetting`
  if (kind === "publishing") return t`Publishing`
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
