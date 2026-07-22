import {
  Button,
  Label,
  ListBox,
  Modal,
  Select,
  Spinner,
  TextArea,
} from "@heroui/react"
import { AlertCircle, ImagePlus, Sparkles } from "lucide-react"
import { useEffect, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { apiJson } from "../../lib/api"
import { randomId } from "../../lib/id"
import {
  isTypesettingDocumentEmpty,
  typesettingCoverRequestPayload,
} from "../../lib/typesetting"
import type {
  AgentSkillListing,
  MyOpenPanelsTransport,
  ProjectTask,
  TypesettingPublication,
} from "../../types"

export function TypesettingCoverTaskDialog({
  isOpen,
  onFlushSave,
  onOpenChange,
  onTaskCreated,
  publication,
  transport,
}: {
  isOpen: boolean
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
      "/api/typesetting/cover-skills"
    )
      .then((response) => {
        if (cancelled) return
        const nextSkills = response.skills ?? []
        setSkills(nextSkills)
        setSelectedSkillId((current) =>
          nextSkills.some((item) => item.skill.id === current)
            ? current
            : (nextSkills.find(
                (item) => item.skill.id === "typesetting-cover-default"
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
        "/api/typesetting/cover-requests",
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(
            typesettingCoverRequestPayload({
              instruction,
              publicationId: publication.id,
              requestId: randomId("cover-request"),
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
        <Modal.Dialog className="op-typesetting-cover-dialog">
          <Modal.CloseTrigger aria-label={t`Close`} />
          <Modal.Header>
            <Modal.Icon>
              <ImagePlus size={19} />
            </Modal.Icon>
            <Modal.Heading>{t`Create cover`}</Modal.Heading>
          </Modal.Header>
          <Modal.Body>
            <div className="op-typesetting-cover-dialog__field">
              <Label>{t`Cover Skill`}</Label>
              {isLoading ? (
                <div className="op-typesetting-cover-dialog__loading">
                  <Spinner size="sm" />
                  <span>{t`Loading Cover Skills`}</span>
                </div>
              ) : skills.length ? (
                <Select
                  aria-label={t`Cover Skill`}
                  onSelectionChange={(key) => setSelectedSkillId(String(key))}
                  selectedKey={selectedSkillId}
                >
                  <Select.Trigger>
                    <Select.Value />
                    <Select.Indicator />
                  </Select.Trigger>
                  <Select.Popover>
                    <ListBox>
                      {skills.map((item) => (
                        <ListBox.Item
                          id={item.skill.id}
                          key={item.skill.id}
                          textValue={item.skill.name}
                        >
                          <div className="op-typesetting-cover-dialog__skill">
                            <strong className="op-typesetting-cover-dialog__skill-name">
                              {item.skill.name}
                            </strong>
                            <span className="op-typesetting-cover-dialog__skill-description">
                              {item.skill.description}
                            </span>
                          </div>
                        </ListBox.Item>
                      ))}
                    </ListBox>
                  </Select.Popover>
                </Select>
              ) : (
                <span className="op-typesetting-cover-dialog__empty">
                  {t`No Cover Skills available`}
                </span>
              )}
            </div>
            <div className="op-typesetting-cover-dialog__field">
              <Label>{t`Additional requirements`}</Label>
              <TextArea
                aria-label={t`Additional requirements`}
                fullWidth
                maxLength={4000}
                onChange={(event) => setInstruction(event.currentTarget.value)}
                placeholder={t`Describe the style, subject, or composition you want`}
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
                (!publication.title.trim() &&
                  isTypesettingDocumentEmpty(publication.content))
              }
              onPress={() => submit().catch(() => undefined)}
              variant="primary"
            >
              {isSubmitting ? <Spinner size="sm" /> : <Sparkles size={15} />}
              {isSubmitting ? t`Submitting` : t`Start creating`}
            </Button>
          </Modal.Footer>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}
