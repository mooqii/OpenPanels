import { useMyOpenPanelsI18n } from "../../canvas"

export function RawDocumentsEmpty() {
  const { t } = useMyOpenPanelsI18n()
  return (
    <div className="op-wiki-module-empty">
      <span>{t`Drag any file type here`}</span>
      <span>{t`to add a document`}</span>
    </div>
  )
}

export function GeneratedDocumentsEmpty() {
  const { t } = useMyOpenPanelsI18n()
  return (
    <div className="op-wiki-module-empty">
      <span>{t`Drag any file type here`}</span>
      <span>{t`to add to My Documents`}</span>
    </div>
  )
}

export function WikiPagesEmpty({
  mentionRawDocuments = true,
}: {
  mentionRawDocuments?: boolean
}) {
  const { t } = useMyOpenPanelsI18n()
  return (
    <div className="op-wiki-module-empty">
      {mentionRawDocuments ? (
        <>
          <span>{t`Content added to Raw Documents`}</span>
          <span>{t`will automatically generate structured Wiki documents`}</span>
        </>
      ) : (
        <span>{t`No structured Wiki documents yet`}</span>
      )}
    </div>
  )
}
