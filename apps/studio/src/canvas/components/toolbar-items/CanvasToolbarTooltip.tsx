import { Tooltip as HeroTooltip } from "@heroui/react"
import type { ComponentProps } from "react"

type CanvasToolbarTooltipProps = ComponentProps<typeof HeroTooltip>

function CanvasToolbarTooltipRoot(props: CanvasToolbarTooltipProps) {
  const { closeDelay: _closeDelay, delay: _delay, ...restProps } = props

  return <HeroTooltip {...restProps} closeDelay={0} delay={0} />
}

export const CanvasToolbarTooltip = Object.assign(
  CanvasToolbarTooltipRoot,
  HeroTooltip
)
