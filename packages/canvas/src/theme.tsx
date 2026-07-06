import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useState,
} from "react"

export type OpenPanelsTheme = "dark" | "light"

export const DEFAULT_OPENPANELS_THEME: OpenPanelsTheme = "dark"
export const OPENPANELS_THEME_COOKIE = "openpanels-theme"

const COOKIE_MAX_AGE_SECONDS = 60 * 60 * 24 * 365
const THEMES: OpenPanelsTheme[] = ["dark", "light"]

interface OpenPanelsThemeContextValue {
  setTheme: (theme: OpenPanelsTheme) => void
  theme: OpenPanelsTheme
}

const OpenPanelsThemeContext =
  createContext<OpenPanelsThemeContextValue | null>(null)

export function isOpenPanelsTheme(
  value: string | null | undefined
): value is OpenPanelsTheme {
  return Boolean(value && THEMES.includes(value as OpenPanelsTheme))
}

export function detectOpenPanelsTheme(): OpenPanelsTheme {
  if (typeof document === "undefined") {
    return DEFAULT_OPENPANELS_THEME
  }

  const cookieTheme = readCookie(OPENPANELS_THEME_COOKIE)
  return isOpenPanelsTheme(cookieTheme) ? cookieTheme : DEFAULT_OPENPANELS_THEME
}

export function applyOpenPanelsTheme(theme: OpenPanelsTheme) {
  if (typeof document === "undefined") return

  const root = document.documentElement
  root.dataset.theme = theme
  root.classList.toggle("dark", theme === "dark")
  root.style.colorScheme = theme
}

export function OpenPanelsThemeProvider({
  children,
  initialTheme,
}: {
  children: ReactNode
  initialTheme?: OpenPanelsTheme
}) {
  const [theme, setTheme] = useState<OpenPanelsTheme>(
    () => initialTheme ?? detectOpenPanelsTheme()
  )

  useLayoutEffect(() => {
    applyOpenPanelsTheme(theme)
  }, [theme])

  useEffect(() => {
    writeCookie(OPENPANELS_THEME_COOKIE, theme, COOKIE_MAX_AGE_SECONDS)
  }, [theme])

  const contextValue = useMemo(
    () => ({
      setTheme,
      theme,
    }),
    [theme]
  )

  return (
    <OpenPanelsThemeContext.Provider value={contextValue}>
      {children}
    </OpenPanelsThemeContext.Provider>
  )
}

export function useOpenPanelsTheme(): OpenPanelsThemeContextValue {
  const context = useContext(OpenPanelsThemeContext)
  if (context) return context

  return {
    setTheme: (_theme: OpenPanelsTheme) => undefined,
    theme: DEFAULT_OPENPANELS_THEME,
  }
}

function readCookie(name: string) {
  if (typeof document === "undefined" || !document.cookie) return undefined
  const cookies = document.cookie.split("; ")
  for (const cookie of cookies) {
    const [key, ...rest] = cookie.split("=")
    if (key === name) {
      return decodeURIComponent(rest.join("="))
    }
  }
  return undefined
}

function writeCookie(name: string, value: string, maxAge: number) {
  if (typeof document === "undefined") return
  document.cookie = `${name}=${encodeURIComponent(value)}; path=/; max-age=${maxAge}; SameSite=Lax`
}
