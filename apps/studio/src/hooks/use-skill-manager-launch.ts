import { useCallback, useState } from "react"

type SkillManagerTab = "installed" | "device" | "add"

export function useSkillManagerLaunch() {
  const [isOpen, setIsOpen] = useState(false)
  const [initialTab, setInitialTab] = useState<SkillManagerTab>("installed")
  const [initialModuleKind, setInitialModuleKind] = useState<string>()
  const [openRequestId, setOpenRequestId] = useState(0)
  const [skillsRevision, setSkillsRevision] = useState(0)

  const open = useCallback(
    (tab: SkillManagerTab = "installed", moduleKind?: string) => {
      setInitialTab(tab)
      setInitialModuleKind(moduleKind)
      setOpenRequestId((current) => current + 1)
      setIsOpen(true)
    },
    []
  )

  const onOpenChange = useCallback((nextIsOpen: boolean) => {
    setIsOpen(nextIsOpen)
    if (!nextIsOpen) setSkillsRevision((current) => current + 1)
  }, [])

  return {
    initialModuleKind,
    initialTab,
    isOpen,
    onOpenChange,
    open,
    openRequestId,
    skillsRevision,
  }
}
