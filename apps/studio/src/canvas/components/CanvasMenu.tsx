import { Button, Dropdown, Header, Label, Separator } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { Blocks, Languages, Menu, Moon, Settings, Sun } from "lucide-react"
import {
  MYOPENPANELS_LOCALE_LABELS,
  type MyOpenPanelsLocale,
  useMyOpenPanelsI18n,
} from "../i18n"
import { type MyOpenPanelsTheme, useMyOpenPanelsTheme } from "../theme"

export function CanvasMenu({
  onOpenModelSettings,
  onOpenSkillManager,
}: {
  onOpenModelSettings?: () => void
  onOpenSkillManager?: () => void
}) {
  const { locale, setLocale } = useMyOpenPanelsI18n()
  const { theme, setTheme } = useMyOpenPanelsTheme()
  const { t } = useLingui()

  return (
    <Dropdown>
      <Button aria-label={t`Open menu`} isIconOnly size="sm" variant="ghost">
        <Menu size={17} strokeWidth={1.8} />
      </Button>
      <Dropdown.Popover className="min-w-52">
        <Dropdown.Menu
          aria-label={t`MyOpenPanels menu`}
          onAction={(key) => {
            const action = String(key)
            if (action.startsWith("locale:")) {
              setLocale(action.replace("locale:", "") as MyOpenPanelsLocale)
              return
            }
            if (action.startsWith("theme:")) {
              setTheme(action.replace("theme:", "") as MyOpenPanelsTheme)
              return
            }
            if (action === "model-settings") {
              onOpenModelSettings?.()
              return
            }
            if (action === "skill-manager") {
              onOpenSkillManager?.()
            }
          }}
        >
          <Dropdown.Section
            selectedKeys={[`locale:${locale}`]}
            selectionMode="single"
          >
            <Header>{t`Language`}</Header>
            <Dropdown.Item id="locale:zh-CN" textValue="简体中文">
              <Languages className="shrink-0 text-muted" size={15} />
              <Dropdown.ItemIndicator />
              <Label className="flex-1">
                {MYOPENPANELS_LOCALE_LABELS["zh-CN"]}
              </Label>
            </Dropdown.Item>
            <Dropdown.Item id="locale:en" textValue="English">
              <Languages className="shrink-0 text-muted" size={15} />
              <Dropdown.ItemIndicator />
              <Label className="flex-1">{MYOPENPANELS_LOCALE_LABELS.en}</Label>
            </Dropdown.Item>
          </Dropdown.Section>
          <Separator />
          <Dropdown.Section
            selectedKeys={[`theme:${theme}`]}
            selectionMode="single"
          >
            <Header>{t`Theme`}</Header>
            <Dropdown.Item id="theme:dark" textValue={t`Dark`}>
              <Moon className="shrink-0 text-muted" size={15} />
              <Dropdown.ItemIndicator />
              <Label className="flex-1">{t`Dark`}</Label>
            </Dropdown.Item>
            <Dropdown.Item id="theme:light" textValue={t`Light`}>
              <Sun className="shrink-0 text-muted" size={15} />
              <Dropdown.ItemIndicator />
              <Label className="flex-1">{t`Light`}</Label>
            </Dropdown.Item>
          </Dropdown.Section>
          {onOpenModelSettings ? (
            <>
              <Separator />
              <Dropdown.Item
                id="model-settings"
                textValue={t`Models and Agents`}
              >
                <Settings className="shrink-0 text-muted" size={15} />
                <Label className="flex-1">{t`Models and Agents`}</Label>
              </Dropdown.Item>
            </>
          ) : null}
          {onOpenSkillManager ? (
            <Dropdown.Item id="skill-manager" textValue={t`Skill management`}>
              <Blocks className="shrink-0 text-muted" size={15} />
              <Label className="flex-1">{t`Skill management`}</Label>
            </Dropdown.Item>
          ) : null}
        </Dropdown.Menu>
      </Dropdown.Popover>
    </Dropdown>
  )
}
