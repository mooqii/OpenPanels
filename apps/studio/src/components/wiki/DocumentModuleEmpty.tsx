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
      <span>{t`While using MyOpenPanels`}</span>
      <span>{t`Agent-generated documents will appear here`}</span>
    </div>
  )
}
