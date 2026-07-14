import { Send } from "lucide-react"
import type { ReactNode } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"

export function PublishingPanel({
  chromeContent,
}: {
  chromeContent: ReactNode
}) {
  const { t } = useMyOpenPanelsI18n()

  return (
    <section className="op-publishing-panel">
      <header className="op-canvas-title">{chromeContent}</header>
      <div className="op-publishing-panel__empty">
        <Send aria-hidden="true" size={24} strokeWidth={1.6} />
        <h1>{t`Publishing`}</h1>
        <p>{t`Publishing is being prepared`}</p>
      </div>
    </section>
  )
}
