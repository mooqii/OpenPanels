import { createContext, useContext } from "react"

import type { NumberInputContextValue } from "./types"

export const NumberInputContext = createContext<NumberInputContextValue | null>(
  null
)

export function useNumberInputContext(): NumberInputContextValue {
  const context = useContext(NumberInputContext)
  if (!context) {
    throw new Error(
      "NumberInput sub-components must be used within <NumberInput>"
    )
  }
  return context
}
