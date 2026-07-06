import { Kbd } from "@heroui/react"

export function Shortcut({ shortcut }: { shortcut: string }) {
  if (!shortcut) return null

  const upperCase = shortcut.toUpperCase()
  return (
    <Kbd className="ml-2" variant="light">
      {upperCase === shortcut ? <Kbd.Abbr keyValue="shift" /> : null}
      <Kbd.Content>{upperCase}</Kbd.Content>
    </Kbd>
  )
}
