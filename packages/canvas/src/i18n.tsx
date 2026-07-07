import { I18nProvider as AriaI18nProvider } from "@react-aria/i18n"
import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react"

export type OpenPanelsLocale = "en" | "zh-CN"

export const DEFAULT_OPENPANELS_LOCALE: OpenPanelsLocale = "en"
export const OPENPANELS_LOCALE_COOKIE = "locale"

const SUPPORTED_LOCALES: OpenPanelsLocale[] = ["en", "zh-CN"]
const COOKIE_MAX_AGE_SECONDS = 60 * 60 * 24 * 365

export const OPENPANELS_LOCALE_LABELS: Record<OpenPanelsLocale, string> = {
  en: "English",
  "zh-CN": "简体中文",
}

const zhCNMessages: Record<string, string> = {
  "Add Image": "添加图片",
  "Apply crop": "应用裁剪",
  "Aspect ratio": "宽高比",
  "Bring to front": "置于顶层",
  Brush: "画刷",
  Blur: "模糊",
  Center: "居中",
  "Click to upload an image": "点击上传图片",
  Colors: "颜色",
  Connector: "连接线",
  Copy: "复制",
  "Color area": "颜色区域",
  "Color field": "颜色输入",
  "Corner Radius": "圆角",
  "Corner radius": "圆角",
  "Create image from selection": "从选区创建图片",
  Crop: "裁剪",
  Custom: "自定义",
  Dark: "深色",
  Delete: "删除",
  Dimensions: "尺寸",
  Download: "下载",
  "Drag to adjust value": "拖动调整数值",
  Ellipse: "椭圆",
  Enabled: "启用",
  "Fill Color": "填充颜色",
  "Fill settings": "填充设置",
  Fill: "填充",
  "Fit to Screen": "适应屏幕",
  "Font Family": "字体",
  "Font Size": "字号",
  "Font Style": "字体样式",
  Gradient: "渐变",
  Group: "组合",
  Hand: "拖动画布",
  "Hue slider": "色相滑块",
  Image: "图片",
  "Image info": "图片信息",
  Inside: "内部",
  Language: "语言",
  Left: "左对齐",
  Light: "浅色",
  Line: "线条",
  Linear: "线性",
  "Link dimensions": "锁定尺寸比例",
  Loading: "加载中",
  "Loading...": "加载中...",
  "Lock aspect ratio": "锁定宽高比",
  Marker: "马克笔",
  Name: "名称",
  Offset: "偏移",
  "Offset X": "X 偏移",
  "Offset Y": "Y 偏移",
  Opacity: "透明度",
  "Open menu": "打开菜单",
  "OpenPanels menu": "OpenPanels 菜单",
  Outside: "外部",
  Paste: "粘贴",
  Pen: "钢笔",
  Pencil: "铅笔",
  "Pencil size medium": "铅笔粗细：中",
  "Pencil size thick": "铅笔粗细：粗",
  "Pencil size thin": "铅笔粗细：细",
  "Pencil sizes": "铅笔粗细",
  "Pick a color": "选择颜色",
  Preset: "预设",
  Projects: "项目",
  "Project name": "项目名称",
  Radial: "径向",
  Rectangle: "矩形",
  "Rename project": "重命名项目",
  Right: "右对齐",
  Rotation: "旋转",
  Scale: "缩放",
  Select: "选择",
  "Send to back": "置于底层",
  Shadow: "阴影",
  "Shadow settings": "阴影设置",
  "Shuffle color": "随机颜色",
  Size: "大小",
  Solid: "纯色",
  Stroke: "描边",
  "Stroke Position": "描边位置",
  "Switch to mixed mode": "切换为分别设置",
  "Switch to uniform mode": "切换为统一设置",
  Text: "文本",
  "Text Alignment": "文本对齐",
  "Text Fill Color": "文本填充颜色",
  Theme: "主题",
  Type: "类型",
  Ungroup: "取消组合",
  "New project": "新建项目",
  "Unlock aspect ratio": "解锁宽高比",
  "Unlink dimensions": "取消锁定尺寸比例",
  Untitled: "未命名",
  Uploading: "上传中",
  "Uploading...": "上传中...",
  "Zoom in": "放大",
  "Zoom out": "缩小",
  "Zoom to 100%": "缩放到 100%",
  "Zoom to 200%": "缩放到 200%",
  "Zoom to 50%": "缩放到 50%",
}

interface OpenPanelsI18nContextValue {
  locale: OpenPanelsLocale
  setLocale: (locale: OpenPanelsLocale) => void
  t: (input: TemplateStringsArray | string, ...values: unknown[]) => string
}

const OpenPanelsI18nContext = createContext<OpenPanelsI18nContextValue | null>(
  null
)

export function isOpenPanelsLocale(
  locale: string | null | undefined
): locale is OpenPanelsLocale {
  return Boolean(
    locale && SUPPORTED_LOCALES.includes(locale as OpenPanelsLocale)
  )
}

export function detectOpenPanelsLocale(): OpenPanelsLocale {
  if (typeof document !== "undefined") {
    const cookieLocale = readCookie(OPENPANELS_LOCALE_COOKIE)
    if (isOpenPanelsLocale(cookieLocale)) {
      return cookieLocale
    }
  }

  if (typeof navigator !== "undefined") {
    const browserLocales = [
      navigator.language,
      ...(navigator.languages ?? []),
    ].filter(Boolean)

    if (
      browserLocales.some((locale) => locale.toLowerCase().startsWith("zh"))
    ) {
      return "zh-CN"
    }
  }

  return DEFAULT_OPENPANELS_LOCALE
}

export function translateOpenPanelsMessage(
  locale: OpenPanelsLocale,
  input: TemplateStringsArray | string,
  ...values: unknown[]
) {
  const message = stringifyTemplate(input, values)
  if (locale === "zh-CN") {
    return zhCNMessages[message] ?? message
  }
  return message
}

export function OpenPanelsI18nProvider({
  children,
  initialLocale,
}: {
  children: ReactNode
  initialLocale?: OpenPanelsLocale
}) {
  const [locale, setLocale] = useState<OpenPanelsLocale>(
    () => initialLocale ?? detectOpenPanelsLocale()
  )

  useEffect(() => {
    writeCookie(OPENPANELS_LOCALE_COOKIE, locale, COOKIE_MAX_AGE_SECONDS)
    document.documentElement.lang = locale
  }, [locale])

  const contextValue = useMemo<OpenPanelsI18nContextValue>(
    () => ({
      locale,
      setLocale,
      t: (input, ...values) =>
        translateOpenPanelsMessage(locale, input, ...values),
    }),
    [locale]
  )

  return (
    <OpenPanelsI18nContext.Provider value={contextValue}>
      <AriaI18nProvider locale={locale}>{children}</AriaI18nProvider>
    </OpenPanelsI18nContext.Provider>
  )
}

export function useOpenPanelsI18n(): OpenPanelsI18nContextValue {
  const context = useContext(OpenPanelsI18nContext)
  if (context) return context

  return {
    locale: DEFAULT_OPENPANELS_LOCALE,
    setLocale: (_locale: OpenPanelsLocale) => undefined,
    t: (input: TemplateStringsArray | string, ...values: unknown[]) =>
      translateOpenPanelsMessage(DEFAULT_OPENPANELS_LOCALE, input, ...values),
  }
}

function stringifyTemplate(
  input: TemplateStringsArray | string,
  values: unknown[]
) {
  if (typeof input === "string") return input
  return input.reduce((result, chunk, index) => {
    const value = index < values.length ? String(values[index]) : ""
    return `${result}${chunk}${value}`
  }, "")
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
