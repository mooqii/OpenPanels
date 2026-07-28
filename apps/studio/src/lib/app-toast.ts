import { ToastQueue } from "@heroui/react"
import type { ReactNode } from "react"

type AppToastContent = {
  description?: ReactNode
  title: ReactNode
  variant?: "default" | "accent" | "success" | "warning" | "danger"
}

type AppToastOptions = Omit<AppToastContent, "title"> & { timeout?: number }

export const appToastQueue = new ToastQueue<AppToastContent>({
  maxVisibleToasts: 3,
})

export function showAppToast(title: ReactNode, options: AppToastOptions = {}) {
  const { timeout = 4000, ...content } = options
  return appToastQueue.add(
    {
      ...content,
      title,
    },
    { timeout }
  )
}
