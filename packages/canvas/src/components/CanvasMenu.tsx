import { Button, Dropdown, Header, Label, Separator } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { Check, Languages, Menu, Moon, Sun } from "lucide-react"
import {
  OPENPANELS_LOCALE_LABELS,
  type OpenPanelsLocale,
  useOpenPanelsI18n,
} from "../i18n"
import { type OpenPanelsTheme, useOpenPanelsTheme } from "../theme"

export function CanvasMenu() {
  const { locale, setLocale } = useOpenPanelsI18n()
  const { theme, setTheme } = useOpenPanelsTheme()
  const { t } = useLingui()

  return (
    <Dropdown>
      <Button
        aria-label={t`Open menu`}
        className="op-canvas-menu-button cursor-pointer"
        isIconOnly
        size="sm"
        variant="ghost"
      >
        <Menu size={17} strokeWidth={1.8} />
      </Button>
      <Dropdown.Popover className="op-canvas-menu-popover min-w-52">
        <Dropdown.Menu
          aria-label={t`OpenPanels menu`}
          onAction={(key) => {
            const action = String(key)
            if (action.startsWith("locale:")) {
              setLocale(action.replace("locale:", "") as OpenPanelsLocale)
              return
            }
            if (action.startsWith("theme:")) {
              setTheme(action.replace("theme:", "") as OpenPanelsTheme)
            }
          }}
        >
          <Dropdown.Section>
            <Header>{t`Language`}</Header>
            <Dropdown.Item id="locale:zh-CN" textValue="简体中文">
              <Languages className="shrink-0 text-muted" size={15} />
              <Label className="flex-1">
                {OPENPANELS_LOCALE_LABELS["zh-CN"]}
              </Label>
              {locale === "zh-CN" ? <Checkmark /> : null}
            </Dropdown.Item>
            <Dropdown.Item id="locale:en" textValue="English">
              <Languages className="shrink-0 text-muted" size={15} />
              <Label className="flex-1">{OPENPANELS_LOCALE_LABELS.en}</Label>
              {locale === "en" ? <Checkmark /> : null}
            </Dropdown.Item>
          </Dropdown.Section>
          <Separator />
          <Dropdown.Section>
            <Header>{t`Theme`}</Header>
            <Dropdown.Item id="theme:dark" textValue={t`Dark`}>
              <Moon className="shrink-0 text-muted" size={15} />
              <Label className="flex-1">{t`Dark`}</Label>
              {theme === "dark" ? <Checkmark /> : null}
            </Dropdown.Item>
            <Dropdown.Item id="theme:light" textValue={t`Light`}>
              <Sun className="shrink-0 text-muted" size={15} />
              <Label className="flex-1">{t`Light`}</Label>
              {theme === "light" ? <Checkmark /> : null}
            </Dropdown.Item>
          </Dropdown.Section>
        </Dropdown.Menu>
      </Dropdown.Popover>
    </Dropdown>
  )
}

function Checkmark() {
  return <Check className="text-accent-soft-foreground" size={15} />
}
