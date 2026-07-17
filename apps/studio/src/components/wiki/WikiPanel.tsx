import {
  useWikiPanelController,
  type WikiPanelProps,
} from "./useWikiPanelController"
import { WikiPanelView } from "./WikiPanelView"

export type { WikiPanelProps } from "./useWikiPanelController"

export function WikiPanel(props: WikiPanelProps) {
  const controller = useWikiPanelController(props)
  return <WikiPanelView {...props} controller={controller} />
}
