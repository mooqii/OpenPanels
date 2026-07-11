import type { ReactNode } from "react"
import { useMyOpenPanelsI18n } from "./i18n"

export function useLingui() {
  const { locale, t } = useMyOpenPanelsI18n()
  return {
    i18n: { locale },
    t,
  }
}

export function Trans({ children }: { children?: ReactNode }) {
  const { t } = useMyOpenPanelsI18n()
  return <>{typeof children === "string" ? t(children) : children}</>
}
