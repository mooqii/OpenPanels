import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useState,
} from "react"

export type MyOpenPanelsTheme = "dark" | "light"

export const DEFAULT_MYOPENPANELS_THEME: MyOpenPanelsTheme = "dark"
export const MYOPENPANELS_THEME_COOKIE = "myopenpanels-theme"

const COOKIE_MAX_AGE_SECONDS = 60 * 60 * 24 * 365
const THEMES: MyOpenPanelsTheme[] = ["dark", "light"]

interface MyOpenPanelsThemeContextValue {
  setTheme: (theme: MyOpenPanelsTheme) => void
  theme: MyOpenPanelsTheme
}

const MyOpenPanelsThemeContext =
  createContext<MyOpenPanelsThemeContextValue | null>(null)

export function isMyOpenPanelsTheme(
  value: string | null | undefined
): value is MyOpenPanelsTheme {
  return Boolean(value && THEMES.includes(value as MyOpenPanelsTheme))
}

export function detectMyOpenPanelsTheme(): MyOpenPanelsTheme {
  if (typeof document === "undefined") {
    return DEFAULT_MYOPENPANELS_THEME
  }

  const cookieTheme = readCookie(MYOPENPANELS_THEME_COOKIE)
  return isMyOpenPanelsTheme(cookieTheme)
    ? cookieTheme
    : DEFAULT_MYOPENPANELS_THEME
}

export function applyMyOpenPanelsTheme(theme: MyOpenPanelsTheme) {
  if (typeof document === "undefined") return

  const root = document.documentElement
  root.dataset.theme = theme
  root.classList.toggle("dark", theme === "dark")
  root.style.colorScheme = theme
}

export function MyOpenPanelsThemeProvider({
  children,
  initialTheme,
}: {
  children: ReactNode
  initialTheme?: MyOpenPanelsTheme
}) {
  const [theme, setTheme] = useState<MyOpenPanelsTheme>(
    () => initialTheme ?? detectMyOpenPanelsTheme()
  )

  useLayoutEffect(() => {
    applyMyOpenPanelsTheme(theme)
  }, [theme])

  useEffect(() => {
    writeCookie(MYOPENPANELS_THEME_COOKIE, theme, COOKIE_MAX_AGE_SECONDS)
  }, [theme])

  const contextValue = useMemo(
    () => ({
      setTheme,
      theme,
    }),
    [theme]
  )

  return (
    <MyOpenPanelsThemeContext.Provider value={contextValue}>
      {children}
    </MyOpenPanelsThemeContext.Provider>
  )
}

export function useMyOpenPanelsTheme(): MyOpenPanelsThemeContextValue {
  const context = useContext(MyOpenPanelsThemeContext)
  if (context) return context

  return {
    setTheme: (_theme: MyOpenPanelsTheme) => undefined,
    theme: DEFAULT_MYOPENPANELS_THEME,
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
