import { Button, Tabs } from "@heroui/react"
import { AlertCircle, LoaderCircle, PanelLeft, Trash2, X } from "lucide-react"
import { useMyOpenPanelsI18n } from "../../canvas"
import type { TypesettingPublication } from "../../types"
import { formatPublicationTime } from "./TypesettingToolbar"

export type PublicationSaveStatus = "saved" | "saving" | "failed"
export type PublicationView = "edit" | "preview"

function PublicationSaveMeta({
  publication,
  saveError,
  saveStatus,
  savedAt = publication.updatedAt,
  onRetrySave,
}: {
  publication: TypesettingPublication
  saveError: string | null
  saveStatus: PublicationSaveStatus
  savedAt?: string
  onRetrySave: () => void
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  return (
    <div className="op-typesetting-detail-header__save-meta">
      <span>
        {t`Last edited`}{" "}
        <time dateTime={savedAt}>{formatPublicationTime(savedAt, locale)}</time>
      </span>
      {saveStatus === "failed" ? (
        <button
          className="is-failed op-typesetting-detail-header__save-state"
          onClick={onRetrySave}
          title={saveError ?? t`Retry save`}
          type="button"
        >
          <AlertCircle size={12} />
          {t`Save failed`}
        </button>
      ) : (
        <span
          className="op-typesetting-detail-header__save-state"
          data-status={saveStatus}
        >
          {saveStatus === "saving" ? (
            <LoaderCircle className="op-spin" size={12} />
          ) : null}
          {saveStatus === "saving" ? t`Saving` : t`Auto-saved`}
        </span>
      )}
    </div>
  )
}

export function PublicationModeHeader({
  onClose,
  onDelete,
  onOpenLibrary,
  onRetrySave,
  onViewChange,
  publication,
  saveError,
  saveStatus,
  view,
}: {
  onClose?: () => void
  onDelete: () => void
  onOpenLibrary?: () => void
  onRetrySave: () => void
  onViewChange: (view: PublicationView) => void
  publication: TypesettingPublication
  saveError: string | null
  saveStatus: PublicationSaveStatus
  view: PublicationView
}) {
  const { t } = useMyOpenPanelsI18n()
  return (
    <div className="op-typesetting-view-header op-typesetting-detail-header op-typesetting-mode-header">
      <PublicationSaveMeta
        onRetrySave={onRetrySave}
        publication={publication}
        saveError={saveError}
        saveStatus={saveStatus}
      />
      {onOpenLibrary ? (
        <Button
          aria-label={t`Open library`}
          className="op-typesetting-mobile-library-button"
          isIconOnly
          onPress={onOpenLibrary}
          size="sm"
          variant="ghost"
        >
          <PanelLeft size={17} />
        </Button>
      ) : null}
      <Tabs
        onSelectionChange={(key) =>
          onViewChange(key === "preview" ? "preview" : "edit")
        }
        selectedKey={view}
      >
        <Tabs.ListContainer>
          <Tabs.List aria-label={t`Publication view`}>
            <Tabs.Tab id="edit">
              {t`Edit`}
              <Tabs.Indicator />
            </Tabs.Tab>
            <Tabs.Tab id="preview">
              {t`Preview`}
              <Tabs.Indicator />
            </Tabs.Tab>
          </Tabs.List>
        </Tabs.ListContainer>
      </Tabs>
      <Button
        aria-label={t`Delete publication project`}
        isIconOnly
        onPress={onDelete}
        size="sm"
        variant="ghost"
      >
        <Trash2 size={15} />
      </Button>
      {onClose ? (
        <Button
          aria-label={t`Close`}
          isIconOnly
          onPress={onClose}
          size="sm"
          variant="ghost"
        >
          <X size={16} />
        </Button>
      ) : null}
    </div>
  )
}

export function PublicationDetailHeader({
  lastSavedAt,
  onDelete,
  onOpenLibrary,
  onPreview,
  onRetrySave,
  publication,
  saveError,
  saveStatus,
}: {
  lastSavedAt: string
  onDelete: () => void
  onOpenLibrary?: () => void
  onPreview: () => void
  onRetrySave: () => void
  publication: TypesettingPublication
  saveError: string | null
  saveStatus: PublicationSaveStatus
}) {
  const { t } = useMyOpenPanelsI18n()
  return (
    <div className="op-typesetting-view-header op-typesetting-detail-header">
      {onOpenLibrary ? (
        <Button
          aria-label={t`Open library`}
          className="op-typesetting-mobile-library-button"
          isIconOnly
          onPress={onOpenLibrary}
          size="sm"
          variant="ghost"
        >
          <PanelLeft size={17} />
        </Button>
      ) : null}
      <PublicationSaveMeta
        onRetrySave={onRetrySave}
        publication={publication}
        savedAt={lastSavedAt}
        saveError={saveError}
        saveStatus={saveStatus}
      />
      <Button
        aria-label={t`Preview`}
        onPress={onPreview}
        size="sm"
        variant="primary"
      >
        {t`Preview`}
      </Button>
      <Button
        aria-label={t`Delete publication project`}
        isIconOnly
        onPress={onDelete}
        size="sm"
        variant="ghost"
      >
        <Trash2 size={15} />
      </Button>
    </div>
  )
}
