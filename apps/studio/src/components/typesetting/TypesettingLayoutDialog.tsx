import {
  Button,
  Label,
  ListBox,
  Modal,
  Select,
  Spinner,
  TextArea,
} from "@heroui/react"
import { AlertCircle, Sparkles } from "lucide-react"
import { useEffect, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { apiJson } from "../../lib/api"
import { randomId } from "../../lib/id"
import {
  isTypesettingDocumentEmpty,
  publicationLayoutRequestPayload,
} from "../../lib/typesetting"
import type {
  AgentSkillListing,
  MyOpenPanelsTransport,
  ProjectTask,
  TypesettingPublication,
} from "../../types"

export function TypesettingLayoutDialog({
  isOpen,
  onManageSkills,
  onFlushSave,
  onOpenChange,
  onTaskCreated,
  publication,
  transport,
}: {
  isOpen: boolean
  onManageSkills?: () => void
  onFlushSave: () => Promise<void>
  onOpenChange: (open: boolean) => void
  onTaskCreated: (task: ProjectTask) => void
  publication: TypesettingPublication
  transport: MyOpenPanelsTransport
}) {
  const { t } = useMyOpenPanelsI18n()
  const [skills, setSkills] = useState<AgentSkillListing[]>([])
  const [selectedSkillId, setSelectedSkillId] = useState("")
  const [instruction, setInstruction] = useState("")
  const [error, setError] = useState<string | null>(null)
  const [isLoading, setIsLoading] = useState(false)
  const [isSubmitting, setIsSubmitting] = useState(false)

  useEffect(() => {
    if (!isOpen) return
    let cancelled = false
    setError(null)
    setIsLoading(true)
    apiJson<{ skills?: AgentSkillListing[] }>(
      transport.apiBase,
      "/api/publications/layout-skills"
    )
      .then((response) => {
        if (cancelled) return
        const nextSkills = response.skills ?? []
        setSkills(nextSkills)
        setSelectedSkillId((current) =>
          nextSkills.some((item) => item.skill.id === current)
            ? current
            : (nextSkills.find(
                (item) => item.skill.id === "publication-layout-default"
              )?.skill.id ??
              nextSkills[0]?.skill.id ??
              "")
        )
      })
      .catch((cause) => {
        if (!cancelled) {
          setError(String(cause instanceof Error ? cause.message : cause))
        }
      })
      .finally(() => {
        if (!cancelled) setIsLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [isOpen, transport.apiBase])

  const submit = async () => {
    if (!selectedSkillId) return
    setIsSubmitting(true)
    setError(null)
    try {
      await onFlushSave()
      const response = await apiJson<{ task: ProjectTask }>(
        transport.apiBase,
        "/api/publications/layout-requests",
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(
            publicationLayoutRequestPayload({
              instruction,
              publicationId: publication.id,
              requestId: randomId("layout-request"),
              skillId: selectedSkillId,
            })
          ),
        }
      )
      onTaskCreated(response.task)
      setInstruction("")
      onOpenChange(false)
    } catch (cause) {
      setError(String(cause instanceof Error ? cause.message : cause))
    } finally {
      setIsSubmitting(false)
    }
  }

  if (!isOpen) return null
  return (
    <Modal.Backdrop
      isOpen
      onOpenChange={(open) => {
        if (!(open || isSubmitting)) onOpenChange(false)
      }}
    >
      <Modal.Container placement="center" size="md">
        <Modal.Dialog className="op-publication-cover-dialog">
          <Modal.CloseTrigger aria-label={t`Close`} />
          <Modal.Header>
            <Modal.Icon>
              <Sparkles size={19} />
            </Modal.Icon>
            <Modal.Heading>{t`Automatic layout`}</Modal.Heading>
          </Modal.Header>
          <Modal.Body>
            <div className="op-publication-cover-dialog__field">
              <div className="op-publication-cover-dialog__field-heading">
                <Label>{t`Layout Skill`}</Label>
                {onManageSkills ? (
                  <Button
                    className="op-publication-cover-dialog__manage-skill"
                    isDisabled={isSubmitting}
                    onPress={() => {
                      onOpenChange(false)
                      onManageSkills()
                    }}
                    size="sm"
                    variant="ghost"
                  >
                    {t`Manage Skill`}
                  </Button>
                ) : null}
              </div>
              {isLoading ? (
                <div className="op-publication-cover-dialog__loading">
                  <Spinner size="sm" />
                  <span>{t`Loading Layout Skills`}</span>
                </div>
              ) : skills.length ? (
                <Select
                  aria-label={t`Layout Skill`}
                  className="op-publication-cover-dialog__select"
                  fullWidth
                  onSelectionChange={(key) => setSelectedSkillId(String(key))}
                  selectedKey={selectedSkillId}
                >
                  <Select.Trigger>
                    <Select.Value />
                    <Select.Indicator />
                  </Select.Trigger>
                  <Select.Popover className="op-publication-cover-dialog__skill-popover">
                    <ListBox>
                      {skills.map((item) => (
                        <ListBox.Item
                          id={item.skill.id}
                          key={item.skill.id}
                          textValue={item.skill.name}
                        >
                          <div className="op-publication-cover-dialog__skill">
                            <strong className="op-publication-cover-dialog__skill-name">
                              {item.skill.name}
                            </strong>
                            <span className="op-publication-cover-dialog__skill-description">
                              {item.skill.description}
                            </span>
                          </div>
                        </ListBox.Item>
                      ))}
                    </ListBox>
                  </Select.Popover>
                </Select>
              ) : (
                <span className="op-publication-cover-dialog__empty">
                  {t`No Layout Skills available`}
                </span>
              )}
            </div>
            <div className="op-publication-cover-dialog__field">
              <Label>{t`Additional requirements`}</Label>
              <TextArea
                aria-label={t`Additional requirements`}
                fullWidth
                maxLength={4000}
                onChange={(event) => setInstruction(event.currentTarget.value)}
                placeholder={t`Describe the layout style or emphasis you want`}
                value={instruction}
              />
            </div>
            {error ? (
              <div className="op-typesetting-inline-error" role="alert">
                <AlertCircle size={15} />
                <span className="op-typesetting-inline-error__message">
                  {error}
                </span>
              </div>
            ) : null}
          </Modal.Body>
          <Modal.Footer>
            <Button
              isDisabled={isSubmitting}
              onPress={() => onOpenChange(false)}
              variant="secondary"
            >
              {t`Cancel`}
            </Button>
            <Button
              isDisabled={
                isLoading ||
                isSubmitting ||
                !selectedSkillId ||
                isTypesettingDocumentEmpty(publication.content)
              }
              onPress={() => submit().catch(() => undefined)}
              variant="primary"
            >
              {isSubmitting ? <Spinner size="sm" /> : <Sparkles size={15} />}
              {isSubmitting ? t`Submitting` : t`Start layout`}
            </Button>
          </Modal.Footer>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}
