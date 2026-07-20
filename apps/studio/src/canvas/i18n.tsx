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
  characters: "字",
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
  "cover image": "封面图",
  "cover images": "封面图",
  "Create image from selection": "从选区创建图片",
  "Convert to Markdown": "转换为 Markdown",
  "Conversion failed": "转换失败",
  "Conversion cancelled": "转换已取消",
  "Content added to Raw Documents": "添加到原始文档的内容",
  Converting: "转换中",
  Crop: "裁剪",
  Custom: "自定义",
  Dark: "深色",
  Cancel: "取消",
  Canvas: "画布",
  Writing: "写作",
  "Writing refinement": "写作提炼",
  Delete: "删除",
  Edit: "编辑",
  "Delete document": "删除文档",
  "Delete document?": "确认删除这个文档？",
  "Delete generated document?": "删除已生成文档？",
  "Delete project": "删除项目",
  "Delete current project": "删除当前项目",
  "Delete project?": "删除项目？",
  Dimensions: "尺寸",
  "Document actions": "文档操作",
  Download: "下载",
  "Drop files to upload": "拖放文件以上传",
  "Drag to adjust value": "拖动调整数值",
  "Drag any file type here": "你可以拖动任意类型文件",
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
  "Index cancelled": "索引已取消",
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
  "Open original file": "打开原始文件",
  "Open in browser": "浏览器里访问",
  "Open menu": "打开菜单",
  "MyOpenPanels menu": "MyOpenPanels 菜单",
  "Model and Agent settings": "模型与 Agent 设置",
  "Models and Agents": "模型与 Agent",
  "Skill management": "Skill 管理",
  "Skill management pages": "Skill 管理页面",
  "Search device Skills": "搜索当前设备 Skill",
  "Clear search": "清除搜索",
  "Add Skill association": "关联 Skill",
  "Add association": "添加关联",
  "After association, this Skill can be called from the selected modules. Make sure the Skill capability matches each module for reliable results.":
    "关联后，可以在所选功能模块中调用这个 Skill。请确认 Skill 的能力与所选模块相匹配，以获得可靠结果。",
  Associated: "已关联",
  "Remove association": "移除关联",
  Save: "保存",
  "Installed Skills": "已安装 Skill",
  "Device Skills": "当前设备 Skill",
  "Add Skill": "添加新 Skill",
  "Install a Skill": "安装 Skill",
  "Choose one source and associate the Skill with a feature module.":
    "选择一个来源，并将 Skill 关联到一个功能模块。",
  "Skill source": "Skill 来源",
  "From URL": "从网址添加",
  "Local upload": "本地上传",
  "Skill URL": "Skill 网址",
  "Supported: GitHub repository and tree URLs, ClawHub Skill detail URLs, and SkillHub.cn Skill detail URLs.":
    "已支持：GitHub 仓库及目录网址、ClawHub Skill 详情网址、SkillHub.cn Skill 详情网址。",
  "Skill folder": "Skill 文件夹",
  "Choose a folder containing SKILL.md": "选择包含 SKILL.md 的文件夹",
  "ZIP package": "ZIP 压缩包",
  "Choose a .zip file": "选择 .zip 文件",
  "Choose Skill folder": "选择 Skill 文件夹",
  "Choose Skill zip": "选择 Skill 压缩包",
  "Associated feature module": "关联功能模块",
  "Select a feature module": "选择功能模块",
  "Required. The Skill will be available from this module after installation.":
    "必选。安装后可以在该功能模块中使用这个 Skill。",
  "Install Skill": "安装 Skill",
  "Validating and installing": "正在校验并安装",
  "Replace existing Skill?": "替换已有 Skill？",
  Replace: "替换",
  "This Skill": "这个 Skill",
  "MyOpenPanels system": "MyOpenPanels 系统",
  System: "系统",
  Preset: "预设",
  "Skill actions": "Skill 操作",
  "Skills found on this device": "当前设备发现的 Skill",
  "Rescan device Skills": "重新扫描设备 Skill",
  "Scanning device Skills": "正在扫描设备 Skill",
  "No device Skills found": "未发现设备 Skill",
  "Skill directory": "Skill 目录",
  Project: "项目",
  Global: "全局",
  "Not available yet": "暂未开放",
  "Delete Skill?": "删除 Skill？",
  "This Skill will be removed from every project.":
    "这个 Skill 将从所有项目中移除。",
  "Task execution": "任务执行",
  "Task processing": "任务处理方式",
  "How tasks are processed": "任务处理方式说明",
  "Enabled Agent CLIs claim queued tasks from left to right in priority order.":
    "已启用的 Agent CLI 会按照从左到右的优先级领取队列中的任务。",
  "Current priority:": "当前优先级：",
  "No enabled automatic task channel.": "没有已启用的自动任务处理通道。",
  "This project can process up to": "当前项目最多可以同时处理",
  "tasks at the same time.": "个任务。",
  "Parallel task count": "并行任务数量",
  parallel: "并行",
  "Scanning available Agent CLIs": "正在扫描可用的 Agent CLI",
  "Running normally": "运行正常",
  Disabled: "已关闭",
  "Connection needs attention": "连接异常",
  "No task-processing model is available. Send each task's instructions to an Agent manually.":
    "没有可用的任务处理模型，你需要手动将每个任务的指令发送给 Agent 处理。",
  "Send task scope to an Agent": "发送任务范围给 Agent",
  "No active and usable Agent CLI is available. Copy the instruction below and send it to an Agent to process this task scope.":
    "当前没有已启用且可正常运行的 Agent CLI。请复制下面的指令并发送给 Agent 来处理这个任务范围。",
  "Task scope instruction": "任务范围指令",
  "Configure CLI": "配置 CLI",
  "Copy instruction": "复制指令",
  "Copy task instruction": "复制任务指令",
  "Copy Project drain instruction": "复制 Project 排空指令",
  Copied: "已复制",
  "Copy failed": "复制失败",
  "Execution mode": "执行方式",
  "Local CLI": "本地 CLI",
  "BYOK API": "BYOK API",
  "Scanning local CLIs": "正在扫描本地 CLI",
  installed: "已安装",
  Rescan: "重新扫描",
  "Not installed": "未安装",
  Connected: "已连接",
  Active: "已启用",
  Inactive: "未启用",
  Activate: "启用",
  Deactivate: "停用",
  Unavailable: "不可用",
  "Drag to reorder": "拖动排序",
  Model: "模型",
  Reasoning: "推理强度",
  "Executable path": "可执行文件路径",
  Test: "测试",
  "Connection successful": "连接成功",
  "Connection failed": "连接失败",
  "BYOK API providers": "BYOK API 提供商",
  "Coming in a later release": "后续版本开放",
  Reserved: "已预留",
  "Save settings": "保存设置",
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
  "to add a document": "到此区域添加文档",
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
  "File name": "文件名",
  "Edit file name": "编辑文件名",
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
  "While using MyOpenPanels": "在使用 MyOpenPanels 期间",
  "Agent-generated documents will appear here": "Agent 生成的文档会出现在这里",
  Generating: "生成中",
  "Generation failed": "生成失败",
  "Generation failed. Click to retry": "生成失败，点击重试",
  "Retry failed. Ask the Agent to generate it again.":
    "重试失败，请让 Agent 重新生成。",
  "Agent work completed": "Agent 工作已完成",
  "Agent work failed": "Agent 工作失败",
  "Not added": "未添加",
  "Original file": "原文件",
  "Preview is not available for this file type": "此文件类型暂不支持预览",
  "Preview original file": "预览原文件",
  "Pending index": "待索引",
  "View related tasks": "查看相关任务",
  Queued: "已排队",
  "Waiting for Agent": "等待 Agent",
  Raw: "原始",
  "Raw Documents": "原始文档",
  Rename: "重命名",
  "Rename generated document": "重命名已生成文档",
  "Re-extract": "重新提取",
  Reindex: "重新索引",
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
  View: "查看",
  "Add document": "添加文档",
  "Add file": "添加文件",
  Uploading: "上传中",
  "Uploading...": "上传中...",
  Wiki: "文档库",
  "Wiki updates": "文档库更新",
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
  "Self-built": "自建",
  "Writing Skill actions": "Writing Skill 操作",
  "Skill files": "Skill 文件",
  "Delete Writing Skill?": "删除 Writing Skill？",
  "After deletion, this Writing Skill can no longer be used.":
    "删除后将无法继续使用这个 Writing Skill。",
  "No Writing Skills available": "没有可用的 Writing Skill",
  "Select at least one Writing Skill": "请至少选择一个 Writing Skill",
  "Revision mode supports one Writing Skill":
    "修订模式只能选择一个 Writing Skill",
  "Start writing": "开始写作",
  Submitting: "提交中",
  "Failed to submit writing request": "写作任务提交失败",
  "Turn selected articles into a Writing Skill":
    "将已选文章提炼为 Writing Skill",
  "The Agent will extract reusable voice, structure, pacing, and techniques from all selected raw and generated documents.":
    "Agent 会从所有已选的原始文档和已生成文档中，提炼可复用的语气、结构、节奏和写作技巧。",
  "Selected articles": "已选文章",
  "Select at least one raw or generated document":
    "请至少选择一篇原始文档或已生成文档",
  "Some selected documents are not ready. Wait for processing or deselect them.":
    "部分已选文档尚未就绪，请等待处理完成或取消选择。",
  "Writing Skill name": "Writing Skill 名称",
  "refinement in progress": "进行中提炼",
  "refinements in progress": "进行中提炼",
  "refinement waiting": "等待中提炼",
  "refinements waiting": "等待中提炼",
  "refinement error": "异常提炼",
  "refinement errors": "异常提炼",
  "Pending creation": "待创作",
  "Pending revision": "待修订",
  "In progress": "进行中",
  "Name this reusable writing method": "为这套可复用的写作方法命名",
  "Start refinement": "开始提炼",
  Refining: "提炼中",
  "Failed to submit refinement request": "提炼任务提交失败",
  "Wiki generation method": "Wiki 生成方式",
  "will automatically generate structured Wiki documents":
    "会自动生成结构化的 Wiki 文档",
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
