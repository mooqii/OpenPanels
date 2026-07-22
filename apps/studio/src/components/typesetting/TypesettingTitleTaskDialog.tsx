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
  typesettingTitleRequestPayload,
} from "../../lib/typesetting"
import type {
  AgentSkillListing,
  MyOpenPanelsTransport,
  ProjectTask,
  TypesettingPublication,
} from "../../types"

export function TypesettingTitleTaskDialog({
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
      "/api/typesetting/title-skills"
    )
      .then((response) => {
        if (cancelled) return
        const nextSkills = response.skills ?? []
        setSkills(nextSkills)
        setSelectedSkillId((current) =>
          nextSkills.some((item) => item.skill.id === current)
            ? current
            : (nextSkills.find(
                (item) => item.skill.id === "typesetting-title-default"
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
        "/api/typesetting/title-requests",
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(
            typesettingTitleRequestPayload({
              instruction,
              publicationId: publication.id,
              requestId: randomId("title-request"),
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
              <Sparkles size={19} />
            </Modal.Icon>
            <Modal.Heading>{t`Generate titles`}</Modal.Heading>
          </Modal.Header>
          <Modal.Body>
            <div className="op-typesetting-cover-dialog__field">
              <Label>{t`Title Skill`}</Label>
              {isLoading ? (
                <div className="op-typesetting-cover-dialog__loading">
                  <Spinner size="sm" />
                  <span>{t`Loading Title Skills`}</span>
                </div>
              ) : skills.length ? (
                <Select
                  aria-label={t`Title Skill`}
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
                  {t`No Title Skills available`}
                </span>
              )}
            </div>
            <div className="op-typesetting-cover-dialog__field">
              <Label>{t`Title requirements`}</Label>
              <TextArea
                aria-label={t`Title requirements`}
                fullWidth
                maxLength={4000}
                onChange={(event) => setInstruction(event.currentTarget.value)}
                placeholder={t`Describe the tone, audience, or style you want`}
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
              {isSubmitting ? t`Submitting` : t`Generate titles`}
            </Button>
          </Modal.Footer>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}
