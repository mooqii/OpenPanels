import type { ReactNode } from "react"
import { useOpenPanelsI18n } from "./i18n"

export function useLingui() {
  const { locale, t } = useOpenPanelsI18n()
  return {
    i18n: { locale },
    t,
  }
}

export function Trans({ children }: { children?: ReactNode }) {
  const { t } = useOpenPanelsI18n()
  return <>{typeof children === "string" ? t(children) : children}</>
}
