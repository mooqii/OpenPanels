import { I18nProvider as AriaI18nProvider } from "@react-aria/i18n"
import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react"

export type MyOpenPanelsLocale = "en" | "zh-CN"

export const DEFAULT_MYOPENPANELS_LOCALE: MyOpenPanelsLocale = "en"
export const MYOPENPANELS_LOCALE_COOKIE = "locale"

const SUPPORTED_LOCALES: MyOpenPanelsLocale[] = ["en", "zh-CN"]
const COOKIE_MAX_AGE_SECONDS = 60 * 60 * 24 * 365

export const MYOPENPANELS_LOCALE_LABELS: Record<MyOpenPanelsLocale, string> = {
  en: "English",
  "zh-CN": "简体中文",
}

const zhCNMessages: Record<string, string> = {
  "Add Image": "添加图片",
  "Agent context": "Agent 上下文",
  Added: "已添加",
  "Add latest version to raw documents": "将最新版本添加到原始文档",
  "Add to raw documents": "添加到原始文档",
  "Apply crop": "应用裁剪",
  "Aspect ratio": "宽高比",
  "Bring to front": "置于顶层",
  Brush: "画刷",
  Blur: "模糊",
  Category: "分类",
  Center: "居中",
  "Click to upload an image": "点击上传图片",
  Claimed: "已领取",
  "Check wiki": "检查 Wiki",
  Close: "关闭",
  Colors: "颜色",
  Connector: "连接线",
  Copy: "复制",
  "Color area": "颜色区域",
  "Color field": "颜色输入",
  "Corner Radius": "圆角",
  "Corner radius": "圆角",
  "Create image from selection": "从选区创建图片",
  "Convert to Markdown": "转换为 Markdown",
  "Conversion failed": "转换失败",
  Converting: "转换中",
  Crop: "裁剪",
  Custom: "自定义",
  Dark: "深色",
  Cancel: "取消",
  Canvas: "画布",
  Writing: "写作",
  Delete: "删除",
  "Delete document": "删除文档",
  "Delete document?": "确认删除这个文档？",
  "Delete generated document?": "删除已生成文档？",
  "Delete project": "删除项目",
  "Delete project?": "删除项目？",
  Dimensions: "尺寸",
  "Document actions": "文档操作",
  Download: "下载",
  "Drop files to upload": "拖放文件以上传",
  "Drag to adjust value": "拖动调整数值",
  Ellipse: "椭圆",
  Enabled: "启用",
  Entity: "实体",
  Failed: "失败",
  "Fill Color": "填充颜色",
  "Fill settings": "填充设置",
  Fill: "填充",
  "Fit to Screen": "适应屏幕",
  "Font Family": "字体",
  "Font Size": "字号",
  "Font Style": "字体样式",
  Gradient: "渐变",
  "Generated Documents": "已生成文档",
  Group: "组合",
  Hand: "拖动画布",
  "Hue slider": "色相滑块",
  Image: "图片",
  "Image info": "图片信息",
  Index: "索引",
  "Index failed": "索引失败",
  Indexed: "已索引",
  Indexing: "索引中",
  Inside: "内部",
  Language: "语言",
  Left: "左对齐",
  Light: "浅色",
  Line: "线条",
  Linear: "线性",
  "Link dimensions": "锁定尺寸比例",
  Loading: "加载中",
  "Loading...": "加载中...",
  Log: "日志",
  "Lock aspect ratio": "锁定宽高比",
  Marker: "马克笔",
  Name: "名称",
  Offset: "偏移",
  "Offset X": "X 偏移",
  "Offset Y": "Y 偏移",
  Opacity: "透明度",
  "Open in new window": "在新窗口打开",
  "Open in browser": "浏览器里访问",
  "Open menu": "打开菜单",
  "MyOpenPanels menu": "MyOpenPanels 菜单",
  Overview: "概览",
  Outside: "外部",
  Page: "页面",
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
  "Project panels": "项目面板",
  "Keep at least one project": "至少保留一个项目",
  Radial: "径向",
  Rectangle: "矩形",
  "Rename project": "重命名项目",
  Right: "右对齐",
  Rotation: "旋转",
  Scale: "缩放",
  Select: "选择",
  "Select for agent context": "选为 Agent 上下文",
  "Send to back": "置于底层",
  Shadow: "阴影",
  "Shadow settings": "阴影设置",
  "Shuffle color": "随机颜色",
  "Show in folder": "在文件夹中显示",
  Size: "大小",
  Solid: "纯色",
  Source: "来源",
  Stroke: "描边",
  "Stroke Position": "描边位置",
  "Switch to mixed mode": "切换为分别设置",
  "Switch to uniform mode": "切换为统一设置",
  Text: "文本",
  "Text Alignment": "文本对齐",
  "Text Fill Color": "文本填充颜色",
  Theme: "主题",
  Topic: "主题",
  "Deleting this project removes all content in the current project, including every Wiki page and everything on the canvas. This cannot be undone.":
    "删除后，当前项目下的所有内容都会被删除，包括文档库、写作任务和画布内容，且不可恢复。",
  "Deleting this project removes its Wiki, writing requests, generated documents, and canvas content. This cannot be undone.":
    "删除后，当前项目下的文档库、写作任务、生成文档和画布内容都会被删除，且不可恢复。",
  "Deleting this project removes its Wiki, writing requests, generated documents, canvas content, and publication projects. This cannot be undone.":
    "删除后，当前项目下的文档库、写作任务、生成文档、画布内容和发布项目都会被删除，且不可恢复。",
  Typesetting: "排版",
  Publishing: "发布",
  "Publishing is being prepared": "发布功能正在准备中",
  "Documents and assets": "文档与素材",
  "Close library": "关闭资料栏",
  Assets: "素材",
  "Asset scope": "素材范围",
  "Current project": "当前项目",
  "All projects": "所有",
  "Loading assets": "正在加载素材",
  "Failed to load assets": "素材加载失败",
  "No Canvas images yet": "Canvas 中还没有图片",
  "Open library": "打开资料栏",
  "Publication content": "发布内容",
  "Manage titles, covers, and details for the current publication content.":
    "管理当前发布内容的标题、封面与详情。",
  New: "新建",
  "Untitled publication": "未命名发布项目",
  "Delete publication project": "删除发布项目",
  "Delete publication project?": "删除发布项目？",
  "This publication project and its layout content will be removed.":
    "此发布项目及其排版内容将被删除。",
  "No publication projects yet": "还没有发布项目",
  "Back to publication projects": "返回发布项目列表",
  Title: "标题",
  Covers: "封面",
  "The first image is used in the project list.":
    "第一张图片将作为列表缩略图。",
  "Drag images from the asset library to add covers.":
    "从左侧素材栏拖入图片以添加封面。",
  "Move cover left": "向前移动封面",
  "Move cover right": "向后移动封面",
  "Remove cover": "删除封面",
  "Content details": "内容详情",
  "Rich text content is saved automatically.": "富文本内容将自动保存。",
  "Open a document from the library and insert it here.":
    "点击左侧文档预览，并一键填充到内容详情。",
  Dismiss: "关闭提示",
  "Text style": "文本样式",
  Paragraph: "正文",
  Bold: "粗体",
  Italic: "斜体",
  "Bullet list": "无序列表",
  "Ordered list": "有序列表",
  "Block quote": "引用",
  Link: "链接",
  "Link URL": "链接地址",
  Apply: "应用",
  Remove: "移除",
  Undo: "撤销",
  Redo: "重做",
  Saving: "保存中",
  Saved: "已保存",
  "Save failed": "保存失败",
  "Retry save": "重试保存",
  "Insert into content details": "插入到内容详情",
  "Loading document": "正在加载文档",
  "Failed to load document": "文档加载失败",
  "Convert this document to Markdown before inserting it.":
    "请先完成 Markdown 转换，再插入此文档。",
  "This raw document will be removed from the source library.":
    "这个原始文档会从源文档库中移除。",
  "This generated document will be removed. Published raw documents will be kept.":
    "此已生成文档将被删除，已经发布的原始文档会保留。",
  Type: "类型",
  Ungroup: "取消组合",
  "New project": "新建项目",
  "Collapse folder": "折叠文件夹",
  "Collapse module": "折叠模块",
  "Expand module": "展开模块",
  "About module": "关于模块",
  "Select Wiki for agent context": "选择文档库作为 Agent 上下文",
  "Source files live here. Added content is converted to Markdown and indexed into the Wiki. Selecting a document lets the agent discover it and load its content when needed.":
    "这里存放源文件。添加的内容会被转换成 Markdown，并索引到文档库中。选中文档后，Agent 会发现该文档，并在需要时加载它的内容。",
  "Structured knowledge pages generated from your sources live here. Agents can search and update this Wiki. Selecting it lets the agent discover the Wiki and load relevant pages when needed.":
    "这里存放从源文档生成的结构化知识页面。Agent 可以搜索和更新文档库。选中后，Agent 会发现这个文档库，并在需要时加载相关页面。",
  "Drafts created by agents live here before they become source material. Agents can create and edit these documents. Selecting a document lets the agent discover it and load its latest content when needed.":
    "这里存放 Agent 创建、尚未加入源文档的草稿。Agent 可以创建和编辑这些文档。选中文档后，Agent 会发现该文档，并在需要时加载它的最新内容。",
  "Expand folder": "展开文件夹",
  "No raw documents yet": "还没有原始文档",
  "No generated documents yet": "还没有已生成文档",
  Generating: "生成中",
  "Generation failed": "生成失败",
  "Agent work completed": "Agent 工作已完成",
  "Agent work failed": "Agent 工作失败",
  "Not added": "未添加",
  "Original file": "原文件",
  "Preview is not available for this file type": "此文件类型暂不支持预览",
  "Preview original file": "预览原文件",
  "Pending index": "待索引",
  Queued: "已排队",
  "Waiting for Agent": "等待 Agent",
  Raw: "原始",
  "Raw Documents": "原始文档",
  Rename: "重命名",
  "Rename generated document": "重命名已生成文档",
  "Re-extract": "重新提取",
  Reindex: "重新索引",
  "Rebuild wiki index": "重建 Wiki 索引",
  Running: "运行中",
  "Save Markdown": "保存 Markdown",
  "Plain text": "纯文本",
  Stale: "已过期",
  "Structured Wiki": "结构化 Wiki",
  Succeeded: "已完成",
  "Unlock aspect ratio": "解锁宽高比",
  "Unlink dimensions": "取消锁定尺寸比例",
  Untitled: "未命名",
  "Update wiki": "更新 Wiki",
  "Upload document": "上传文档",
  Uploading: "上传中",
  "Uploading...": "上传中...",
  Wiki: "文档库",
  "Writing mode": "写作模式",
  "New document": "新建文档",
  Revise: "修订",
  Refine: "提炼",
  "Document to revise": "要修订的文档",
  "Select a generated document": "选择已生成文档",
  "Writing instructions": "写作要求",
  "Describe what the agent should write": "描述希望 Agent 写作或修订的内容",
  "Writing Skills": "Writing Skills",
  "Select one": "选择一个",
  "Select one or more": "选择一个或多个",
  "Select Writing Skill": "选择 Writing Skill",
  "Built-in": "内置",
  Project: "项目",
  "No Writing Skills available": "没有可用的 Writing Skill",
  "Select at least one Writing Skill": "请至少选择一个 Writing Skill",
  "Revision mode supports one Writing Skill":
    "修订模式只能选择一个 Writing Skill",
  "Start writing": "开始写作",
  Submitting: "提交中",
  "Failed to submit writing request": "写作任务提交失败",
  "Writing request": "写作任务",
  "No writing requests yet": "还没有写作任务",
  "Turn selected articles into a Writing Skill":
    "将已选文章提炼为 Writing Skill",
  "The Agent will extract reusable voice, structure, pacing, and techniques from all selected raw and generated documents.":
    "Agent 会从所有已选的原始文档和已生成文档中，提炼可复用的语气、结构、节奏和写作技巧。",
  "The selected Wiki is ignored. Raw documents must be converted to Markdown. When complete, the project Skill will be available for new documents and revisions.":
    "提炼不会使用已选文档库。原始文档必须已转换为 Markdown；完成后，项目 Skill 可用于新建和修订文档。",
  "Selected articles": "已选文章",
  "Select at least one raw or generated document":
    "请至少选择一篇原始文档或已生成文档",
  "Some selected documents are not ready. Wait for processing or deselect them.":
    "部分已选文档尚未就绪，请等待处理完成或取消选择。",
  "All selected articles will be refined together": "所有已选文章将合并提炼",
  "Writing Skill name": "Writing Skill 名称",
  "Name this reusable writing method": "为这套可复用的写作方法命名",
  "Start refinement": "开始提炼",
  Refining: "提炼中",
  "Failed to submit refinement request": "提炼任务提交失败",
  "Writing Skill refinement": "Writing Skill 提炼任务",
  "No refinement requests yet": "还没有提炼任务",
  "Cancel refinement request": "取消提炼任务",
  "Retry refinement request": "重试提炼任务",
  "Cancel writing request": "取消写作任务",
  "Retry writing request": "重试写作任务",
  Completed: "已完成",
  Cancelled: "已取消",
  "Wiki generation method": "Wiki 生成方式",
  "Zoom in": "放大",
  "Zoom out": "缩小",
  "Zoom to 100%": "缩放到 100%",
  "Zoom to 200%": "缩放到 200%",
  "Zoom to 50%": "缩放到 50%",
}

interface MyOpenPanelsI18nContextValue {
  locale: MyOpenPanelsLocale
  setLocale: (locale: MyOpenPanelsLocale) => void
  t: (input: TemplateStringsArray | string, ...values: unknown[]) => string
}

const MyOpenPanelsI18nContext =
  createContext<MyOpenPanelsI18nContextValue | null>(null)

export function isMyOpenPanelsLocale(
  locale: string | null | undefined
): locale is MyOpenPanelsLocale {
  return Boolean(
    locale && SUPPORTED_LOCALES.includes(locale as MyOpenPanelsLocale)
  )
}

export function detectMyOpenPanelsLocale(): MyOpenPanelsLocale {
  if (typeof document !== "undefined") {
    const cookieLocale = readCookie(MYOPENPANELS_LOCALE_COOKIE)
    if (isMyOpenPanelsLocale(cookieLocale)) {
      return cookieLocale
    }
  }

  if (typeof navigator !== "undefined") {
    const browserLocales = [
      navigator.language,
      ...(navigator.languages ?? []),
    ].filter(Boolean)

    return localeFromBrowserLanguages(browserLocales)
  }

  return DEFAULT_MYOPENPANELS_LOCALE
}

export function localeFromBrowserLanguages(
  locales: string[]
): MyOpenPanelsLocale {
  return locales.some((locale) => locale.toLowerCase().startsWith("zh"))
    ? "zh-CN"
    : DEFAULT_MYOPENPANELS_LOCALE
}

export function translateMyOpenPanelsMessage(
  locale: MyOpenPanelsLocale,
  input: TemplateStringsArray | string,
  ...values: unknown[]
) {
  const message = stringifyTemplate(input, values)
  if (locale === "zh-CN") {
    return zhCNMessages[message] ?? message
  }
  return message
}

export function MyOpenPanelsI18nProvider({
  children,
  initialLocale,
}: {
  children: ReactNode
  initialLocale?: MyOpenPanelsLocale
}) {
  const [locale, setLocale] = useState<MyOpenPanelsLocale>(
    () => initialLocale ?? detectMyOpenPanelsLocale()
  )

  useEffect(() => {
    writeCookie(MYOPENPANELS_LOCALE_COOKIE, locale, COOKIE_MAX_AGE_SECONDS)
    document.documentElement.lang = locale
  }, [locale])

  const contextValue = useMemo<MyOpenPanelsI18nContextValue>(
    () => ({
      locale,
      setLocale,
      t: (input, ...values) =>
        translateMyOpenPanelsMessage(locale, input, ...values),
    }),
    [locale]
  )

  return (
    <MyOpenPanelsI18nContext.Provider value={contextValue}>
      <AriaI18nProvider locale={locale}>{children}</AriaI18nProvider>
    </MyOpenPanelsI18nContext.Provider>
  )
}

export function useMyOpenPanelsI18n(): MyOpenPanelsI18nContextValue {
  const context = useContext(MyOpenPanelsI18nContext)
  if (context) return context

  return {
    locale: DEFAULT_MYOPENPANELS_LOCALE,
    setLocale: (_locale: MyOpenPanelsLocale) => undefined,
    t: (input: TemplateStringsArray | string, ...values: unknown[]) =>
      translateMyOpenPanelsMessage(
        DEFAULT_MYOPENPANELS_LOCALE,
        input,
        ...values
      ),
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
