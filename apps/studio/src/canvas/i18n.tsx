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
  "Agent generated": "Agent 生成",
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
  Crop: "裁剪",
  Custom: "自定义",
  Dark: "深色",
  Cancel: "取消",
  Canvas: "画布",
  "Confirm another publishing attempt": "确认再次发布",
  "Continue with current Agent": "继续交给当前 Agent",
  "Confirm publish": "确认发布",
  "Create content in Typesetting before publishing": "请先在排版中创建内容",
  "Create handoff": "创建交接",
  "Current browser account": "浏览器当前登录账号",
  Writing: "写作",
  "Writing distillation": "写作提炼",
  Delete: "删除",
  Done: "完成",
  Edit: "编辑",
  "Delete document": "删除文档",
  "Delete document?": "确认删除这个文档？",
  "Delete My Document?": "删除我的文档？",
  "Delete project": "删除项目",
  "Delete current project": "删除当前项目",
  "Delete project?": "删除项目？",
  Dimensions: "尺寸",
  "Document actions": "文档操作",
  Download: "下载",
  "Drop files to upload": "拖放文件以上传",
  "Drag to adjust value": "拖动调整数值",
  "Drag any file type here": "你可以拖动任意类型文件",
  "to add to My Documents": "到此区域添加到我的文档",
  Ellipse: "椭圆",
  Enabled: "启用",
  Entity: "实体",
  Failed: "失败",
  Cancelled: "已取消",
  "Fill Color": "填充颜色",
  "Fill settings": "填充设置",
  Fill: "填充",
  "Fit to Screen": "适应屏幕",
  "Font Family": "字体",
  "Font Size": "字号",
  "Font Style": "字体样式",
  Gradient: "渐变",
  "My Documents": "我的文档",
  Group: "组合",
  Hand: "拖动画布",
  "Hue slider": "色相滑块",
  Image: "图片",
  "Image info": "图片信息",
  Index: "索引",
  Indexed: "已索引",
  Indexing: "索引中",
  Covered: "已覆盖",
  Filtered: "已筛除",
  Inside: "内部",
  Language: "语言",
  Left: "左对齐",
  Light: "浅色",
  Line: "线条",
  Linear: "线性",
  "Link dimensions": "锁定尺寸比例",
  Loading: "加载中",
  "Loading...": "加载中...",
  "Loading Preset Skills": "正在加载预设 Skill",
  Log: "日志",
  "Lock aspect ratio": "锁定宽高比",
  Marker: "马克笔",
  Name: "名称",
  "Needs user action": "需要用户操作",
  "No content selected": "未选择内容",
  "No publishing attempts yet": "暂无发布记录",
  "Not published": "未发布",
  Offset: "偏移",
  "Offset X": "X 偏移",
  "Offset Y": "Y 偏移",
  Opacity: "透明度",
  "Open in new window": "在新窗口打开",
  "Open original file": "打开原始文件",
  "Open in browser": "浏览器里访问",
  "Open published note": "打开已发布笔记",
  "Open task": "打开任务",
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
  "Choose a source, review the Skills found, and configure module associations.":
    "选择来源，查看发现的 Skill，并配置功能模块关联。",
  "Skill source": "Skill 来源",
  "Recommended Skills": "推荐 Skill",
  "No recommended Skills yet": "暂无推荐 Skill",
  "Recommended Skills will appear here in a future app update.":
    "后续应用更新中提供的推荐 Skill 会显示在这里。",
  "From URL": "从网址添加",
  "Local upload": "本地上传",
  "Skill URL": "Skill 网址",
  "Scan URL": "扫描网址",
  Scanning: "正在扫描",
  "Choose a feature module for each Skill, or leave it unassociated.":
    "为每个 Skill 选择关联的功能模块，也可以选择不关联。",
  "Choose a feature module for each Skill before installation.":
    "安装前必须为每个 Skill 选择关联的功能模块。",
  "Do not associate": "不关联",
  "Install Skills": "安装 Skill",
  "Unassociated Skills": "未关联的 Skill",
  "Supported: GitHub repository and tree URLs, skills.sh repository and Skill detail URLs, ClawHub Skill detail URLs, and SkillHub.cn Skill detail URLs.":
    "已支持：GitHub 仓库及目录网址、skills.sh 仓库及 Skill 详情网址、ClawHub Skill 详情网址、SkillHub.cn Skill 详情网址。",
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
  Installing: "正在安装",
  Installed: "已安装",
  Install: "安装",
  "Skill conflict": "Skill 冲突",
  "Module association needed": "需要关联模块",
  "Associate modules": "关联模块",
  "Installed Skill could not be found.": "找不到已安装的 Skill。",
  "The Skill was updated, but its recommended module associations could not be applied.":
    "Skill 已更新，但无法应用推荐的功能模块关联。",
  "Replace existing Skill?": "替换已有 Skill？",
  Replace: "替换",
  "This Skill": "这个 Skill",
  "MyOpenPanels system": "MyOpenPanels 系统",
  System: "系统",
  Preset: "预设",
  "Skill actions": "Skill 操作",
  "Adjust modules": "调整关联模块",
  "Adjust Skill modules": "调整 Skill 关联模块",
  "Skills found on this device": "当前设备发现的 Skill",
  "Rescan device Skills": "重新扫描设备 Skill",
  "Check Skill updates": "检查 Skill 更新",
  Checking: "正在检查",
  "Not checked": "尚未检查",
  "Updates unavailable": "不支持更新",
  "Source unavailable": "来源不可用",
  "Source unavailable · Local changes": "来源不可用 · 有本地修改",
  "Update available": "有可用更新",
  "Update available · Local changes": "有可用更新 · 有本地修改",
  "Local changes": "有本地修改",
  "Up to date": "已是最新",
  "Local import": "本地导入",
  Device: "设备",
  "Update Skill": "更新 Skill",
  "Update Skill with local changes?": "覆盖本地修改并更新 Skill？",
  "Discard changes and update": "放弃修改并更新",
  "Local edits to this Skill will be permanently discarded and replaced with the latest source version.":
    "这个 Skill 的本地修改将被永久丢弃，并替换为来源中的最新版本。",
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
  "Task channel": "任务通道",
  Tasks: "任务",
  Communication: "通信",
  "MyOpenPanels Agent panel": "MyOpenPanels Agent 面板",
  "Agent panel pages": "Agent 面板页面",
  "Close Agent panel": "关闭 Agent 面板",
  "Clear communication view": "清空通信记录",
  "Communication event types": "通信事件类型",
  "No communication events in this view.": "当前视图中没有通信事件。",
  Latest: "最新",
  all: "全部",
  cli: "CLI",
  agent: "Agent",
  api: "API",
  task: "任务",
  system: "系统",
  error: "错误",
  Automatic: "自动选择",
  Prefer: "优先使用",
  "Send instruction manually": "需手动发送指令",
  "Manual instruction": "手动指令",
  "Archive task": "归档任务",
  "Delete task": "删除任务",
  "Delete task?": "删除这个任务？",
  "This task and any dependent tasks will be removed. The related source status will be updated.":
    "这个任务及其后置任务将被移除，关联对象的状态也会同步更新。",
  "Copy task detail": "复制任务详情",
  "No project tasks yet.": "当前项目还没有任务。",
  "No project tasks.": "没有项目任务。",
  "No pending tasks.": "没有待处理任务。",
  "No active tasks.": "没有进行中的任务。",
  "No closed tasks.": "没有已结束任务。",
  "Task failed": "任务失败",
  "Back to all tasks": "返回全部任务",
  "All tasks": "全部任务",
  "Pending tasks": "待处理任务",
  "Active tasks": "进行中任务",
  "Closed tasks": "已结束任务",
  "Distillation tasks": "提炼任务",
  All: "全部",
  Closed: "已结束",
  queued: "等待中",
  running: "进行中",
  succeeded: "已完成",
  failed: "失败",
  cancelled: "已取消",
  superseded: "已取代",
  "not ready": "尚未就绪",
  exhausted: "重试次数已用尽",
  "retry later": "稍后重试",
  leased: "已被领取",
  "Next run": "下次运行",
  "Lease until": "领取有效期至",
  attempt: "尝试次数",
  Prerequisites: "前置任务",
  Execution: "执行信息",
  generation: "执行代次",
  "compatible targets": "兼容目标数",
  eligible: "可调度",
  "no target": "无可用目标",
  done: "已完成",
  fail: "失败时终止",
  skip: "失败时跳过",
  writing: "写作",
  publication: "出版",
  typesetting: "排版",
  canvas: "画布",
  wiki: "Wiki",
  "Generate Writing Document": "生成写作文档",
  "Write My Document": "生成我的文档",
  "Distill Writing Skill": "提炼写作 Skill",
  "Import Markdown into Wiki": "将 Markdown 导入 Wiki",
  "Update Wiki": "更新 Wiki",
  "Generate Publication Cover": "生成出版封面",
  "Generate Publication Titles": "生成出版标题",
  "Format Publication Content": "排版出版内容",
  "Task processing": "任务处理方式",
  "How tasks are processed": "任务处理方式说明",
  "Enabled Agent CLIs claim queued tasks from left to right in priority order.":
    "已启用的 Agent CLI 会按照从左到右的优先级领取队列中的任务。",
  "Current priority:": "当前优先级：",
  "No enabled automatic task channel.": "没有已启用的自动任务处理通道。",
  "No active Agent CLI is currently available.":
    "当前没有激活的 Agent CLI 可用。",
  "Use the settings button on the right to configure the Agent CLI that processes tasks.":
    "你可以通过右侧的设置按钮配置处理任务的 Agent CLI。",
  "Until an Agent CLI is activated, copy the task instruction manually and send it to an Agent.":
    "在激活 Agent CLI 前，请手动复制任务指令并发送给 Agent。",
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
  "Send Task Handoff to an Agent": "发送 Task Handoff 给 Agent",
  "No active and usable Agent CLI is available. Copy the instruction below and send it to an Agent to run this Task Handoff.":
    "当前没有已启用且可正常运行的 Agent CLI。请复制下面的指令并发送给 Agent 来执行这个 Task Handoff。",
  "Task Handoff instruction": "Task Handoff 指令",
  "Configure CLI": "配置 CLI",
  "Copy instruction": "复制指令",
  "Copy Agent message": "复制 Agent 消息",
  "Agent message copied": "Agent 消息已复制",
  "Copy task instruction": "复制任务指令",
  "Copy Wiki update instruction": "复制 Wiki 更新指令",
  "Copy Project drain instruction": "复制 Project 排空指令",
  "Please send the instruction to an Agent manually": "请手动发送指令给 Agent",
  "Waiting for an active Agent CLI to claim the task":
    "待连接的 Agent CLI 领取任务",
  subtasks: "个子任务",
  subtask: "个子任务",
  "waiting for document conversion": "等待文档转换",
  "waiting for earlier update": "等待前序更新",
  "Document conversion": "文档转换",
  "Agent is processing Wiki update tasks": "Agent 正在处理 Wiki 更新任务",
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
  "Deleting this project removes its Wiki, writing requests, My Documents, and canvas content. This cannot be undone.":
    "删除后，当前项目下的文档库、写作任务、我的文档和画布内容都会被删除，且不可恢复。",
  "Deleting this project removes its Wiki, writing requests, My Documents, canvas content, and publication projects. This cannot be undone.":
    "删除后，当前项目下的文档库、写作任务、我的文档、画布内容和发布项目都会被删除，且不可恢复。",
  Typeset: "排版",
  "Content publishing": "内容发布",
  "Cover creation": "封面制作",
  "Title generation": "标题生成",
  "Publishing status": "发布状态",
  "Add content publishing Skill": "添加内容发布 Skill",
  Publish: "发布",
  Start: "启动",
  "Start publishing task?": "启动发布任务？",
  "Confirm start": "确认启动",
  "No publishing tasks yet": "还没有发布任务",
  "No content publishing Skills installed": "还没有安装内容发布 Skill",
  "This Skill is no longer installed": "这个 Skill 已不再安装",
  "Add text content or at least one image to publish":
    "正文或图片至少填写一项后即可发布",
  "The images will be used in order and the Agent will perform the final publishing action once.":
    "将按顺序使用图片，Agent 只会执行一次最终发布操作。",
  "The text content will be used and the Agent will perform the final publishing action once.":
    "将使用当前文字内容，Agent 只会执行一次最终发布操作。",
  "The previous attempt may already have published. Check the target platform before continuing.":
    "上次操作可能已经发布，请先检查目标平台再继续。",
  "Publishing history": "发布历史",
  "Publishing in progress": "发布进行中",
  "Publishing Skill": "发布 Skill",
  "Publish now": "立即发布",
  "Publish preview": "发布预览",
  "Back to publish preview": "返回发布预览",
  "Publish settings": "发布设置",
  "Publish to Xiaohongshu now?": "立即发布到小红书？",
  Published: "已发布",
  published: "已发布",
  "Pending publish": "待发布",
  "Publishing now": "发布中",
  "Publishing error": "发布错误",
  "Publishing status unknown": "状态未知",
  Queued: "排队中",
  "Read only": "只读",
  "Result unknown": "结果未知",
  "Retry automatically": "自动重试",
  Running: "执行中",
  "Hand off to current Agent": "交给当前 Agent",
  "Primary cover": "主封面",
  "Typesetting content": "排版内容",
  "Open Typesetting content": "展开排版内容列表",
  "Close Typesetting content": "收起排版内容列表",
  "Open publication content": "展开发布内容列表",
  "Close publication content": "收起发布内容列表",
  "Xiaohongshu image note": "小红书图文笔记",
  "Xiaohongshu publishing": "小红书发布",
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
  "Publication view": "发布内容视图",
  Preview: "预览",
  "Manage titles, covers, and details for the current publication content.":
    "管理当前发布内容的标题、封面与详情。",
  "Select publication content from the left to edit, or create new publication content.":
    "从左侧选择一个发布内容进行编辑，或者新建发布内容。",
  New: "新建",
  "Untitled publication": "未命名发布项目",
  "Delete publication project": "删除发布项目",
  "Delete publication project?": "删除发布项目？",
  "This publication project and its layout content will be removed.":
    "此发布项目及其排版内容将被删除。",
  "No publication projects yet": "还没有发布项目",
  "Back to publication projects": "返回发布项目列表",
  Title: "标题",
  Titles: "标题",
  "Add title": "添加标题",
  "Collapse titles": "收起候选标题",
  "Delete title": "删除标题",
  "Expand titles": "展开候选标题",
  "Generate titles": "生成标题",
  "Generating titles": "正在生成标题",
  "Loading Title Skills": "正在加载标题 Skill",
  "New title": "新建标题",
  "No Title Skills available": "没有可用的标题 Skill",
  "Saving titles": "正在保存标题",
  "Title generation cancelled": "标题生成已取消",
  "Title generation failed": "标题生成失败",
  "Title requirements": "标题要求",
  "Title Skill": "标题 Skill",
  "Waiting for titles": "等待生成标题",
  "Describe the tone, audience, or style you want":
    "描述希望使用的语气、受众或标题风格",
  Tags: "标签",
  "Add tag": "添加标签",
  Covers: "封面",
  Add: "添加",
  "Add cover images": "添加封面图片",
  "Add cover image source": "封面图片来源",
  "Image source": "图片来源",
  "Insert images": "插入图片",
  "Insert selected images": "插入所选图片",
  "From Canvas": "从画布添加",
  Upload: "上传",
  "Add selected images": "添加所选图片",
  "Failed to add some images": "部分图片添加失败",
  "Failed to upload some images": "部分图片上传失败",
  "Drag or paste images here": "拖动或粘贴图片到这里",
  "Uploading images": "正在上传图片",
  "Upload images": "上传图片",
  "Create cover": "制作封面",
  "Cover Skill": "封面 Skill",
  "Loading Cover Skills": "正在加载封面 Skill",
  "No Cover Skills available": "没有可用的封面 Skill",
  "Additional requirements": "额外要求",
  "Describe the style, subject, or composition you want":
    "描述希望使用的风格、主体或构图",
  "Start creating": "开始制作",
  "Waiting to create": "等待制作",
  "Creating cover": "正在制作封面",
  "Saving cover": "正在保存封面",
  "Cover creation failed": "封面制作失败",
  "Cover creation cancelled": "封面制作已取消",
  "The first image is used in the project list.":
    "第一张图片将作为列表缩略图。",
  "Drag Canvas assets or image files here to add covers.":
    "从画布素材或电脑文件夹拖入图片以添加封面。",
  "Move cover left": "向前移动封面",
  "Move cover right": "向后移动封面",
  "Remove cover": "删除封面",
  "Content details": "内容详情",
  "Automatic layout": "自动排版",
  "Layout Skill": "排版 Skill",
  "Loading Layout Skills": "正在加载排版 Skill",
  "No Layout Skills available": "没有可用的排版 Skill",
  "Describe the layout style or emphasis you want":
    "描述希望使用的排版风格或重点",
  "Start layout": "开始排版",
  "Waiting for layout": "等待排版",
  "Formatting content": "正在排版",
  "Layout completed": "排版完成",
  "Layout failed": "排版失败",
  "Layout cancelled": "排版已取消",
  "A layout task is in progress. Cancel it or wait for it to finish before editing.":
    "当前有一个排版中任务，取消或完成后才可以继续编辑。",
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
  "Align left": "左对齐",
  "Align center": "居中对齐",
  "Align right": "右对齐",
  Link: "链接",
  "Link URL": "链接地址",
  Apply: "应用",
  Remove: "移除",
  Undo: "撤销",
  Redo: "重做",
  Saving: "保存中",
  Saved: "已保存",
  "Auto-saved": "已自动保存",
  "Last edited": "最后编辑",
  "Save failed": "保存失败",
  "Retry save": "重试保存",
  "Insert into content details": "插入到内容详情",
  "Insert document content into content details": "将文档内容插入到内容详情",
  "Generate publication content from this document": "根据此文档生成发布内容",
  "Click to generate new publication content from this document.":
    "点击后将根据当前文档生成一篇新的发布内容。",
  "Loading document": "正在加载文档",
  "File name": "文件名",
  "Edit file name": "编辑文件名",
  "Failed to load document": "文档加载失败",
  "Convert this document to Markdown before inserting it.":
    "请先完成 Markdown 转换，再插入此文档。",
  "This raw document will be removed from the source library.":
    "这个原始文档会从源文档库中移除。",
  "This My Document will be removed. Published raw documents will be kept.":
    "此我的文档将被删除，已经发布的原始文档会保留。",
  "This document will be removed from My Documents. Published raw documents will be kept.":
    "此文档将从我的文档中删除，已经发布的原始文档会保留。",
  "This document will be removed from My Documents.":
    "此文档将从我的文档中删除。",
  "All generated Wiki pages in this project will be deleted and rebuilt with the selected Skill. Raw documents and My Documents will be kept.":
    "当前项目中的所有结构化 Wiki 页面都将删除，并使用所选 Skill 重建。原始文档和我的文档会保留。",
  "All generated Wiki pages in this project will be deleted and rebuilt with the selected Skill. My Documents will be kept.":
    "当前项目中的所有结构化 Wiki 页面都将删除，并使用所选 Skill 重建。我的文档会保留。",
  Type: "类型",
  Ungroup: "取消组合",
  "New project": "新建项目",
  "Collapse folder": "折叠文件夹",
  "Collapse module": "折叠模块",
  "Expand module": "展开模块",
  "About module": "关于模块",
  "Select Wiki for agent context": "选择 Wiki 作为 Agent 上下文",
  "Source files live here. Added content is converted to Markdown and indexed into the Wiki. Selecting a document lets the agent discover it and load its content when needed.":
    "这里存放源文件。添加的内容会被转换成 Markdown，并索引到 Wiki 中。选中文档后，Agent 会发现该文档，并在需要时加载它的内容。",
  "Source files live here. Added content is converted to Markdown and indexed into the Wiki.":
    "这里存放源文件。添加的内容会被转换成 Markdown，并索引到 Wiki 中。",
  "Structured knowledge pages generated from your sources live here. Agents can search and update this Wiki. Selecting it lets the agent discover the Wiki and load relevant pages when needed.":
    "这里存放从源文档生成的结构化知识页面。Agent 可以搜索和更新 Wiki。选中后，Agent 会发现这个 Wiki，并在需要时加载相关页面。",
  "Structured knowledge pages generated from your sources live here. Agents can search and update this Wiki.":
    "这里存放从源文档生成的结构化知识页面。Agent 可以搜索和更新 Wiki。",
  "Drafts created by agents live here before they become source material. Agents can create and edit these documents. Selecting a document lets the agent discover it and load its latest content when needed.":
    "这里存放 Agent 创建、尚未加入源文档的草稿。Agent 可以创建和编辑这些文档。选中文档后，Agent 会发现该文档，并在需要时加载它的最新内容。",
  "Documents you add or create with agents live here. Imported files are converted when needed without changing the Wiki. Selecting a document lets the agent discover it and load its latest content.":
    "这里存放你添加或与 Agent 一起创建的文档。导入的文件会在需要时转换，且不会自动更改 Wiki。选中文档后，Agent 会发现它，并在需要时加载最新内容。",
  "Expand folder": "展开文件夹",
  "No raw documents yet": "还没有原始文档",
  "No My Documents yet": "我的文档中还没有内容",
  "While using MyOpenPanels": "在使用 MyOpenPanels 期间",
  "My Documents created by the Agent will appear here":
    "Agent 创建的文档会出现在这里",
  Generating: "生成中",
  Converting: "转换中",
  "Pending conversion": "等待转换",
  "Conversion failed. Click to retry": "转换失败，点击重试",
  "My Document write failed": "文档写入失败",
  "My Document write failed. Click to retry": "生成失败，点击重试",
  "Retry failed. Ask the Agent to generate it again.":
    "重试失败，请让 Agent 重新生成。",
  "Retry failed. Ask the Agent to try again.": "重试失败，请让 Agent 重试。",
  "Retry task": "重试任务",
  "Retry queued": "已创建重试",
  "New retry task": "新重试任务",
  "Agent work completed": "Agent 工作已完成",
  "Agent work failed": "Agent 工作失败",
  "Not added": "未添加",
  "Original file": "原文件",
  "Preview is not available for this file type": "此文件类型暂不支持预览",
  "Preview original file": "预览原文件",
  Pending: "等待中",
  Unrecorded: "未记录",
  Unscheduled: "未调度",
  "View related tasks": "查看相关任务",
  "Waiting for Agent": "等待 Agent",
  Raw: "原始",
  "Raw Documents": "原始文档",
  Rename: "重命名",
  "Rename My Document": "重命名我的文档",
  "Rename document": "重命名文档",
  "Rename raw document": "重命名原始文档",
  "Re-extract": "重新提取",
  Reindex: "重新索引",
  "Save Markdown": "保存 Markdown",
  "Plain text": "纯文本",
  Stale: "已过期",
  "Structured Wiki": "结构化 Wiki",
  Succeeded: "已完成",
  "Unlock aspect ratio": "解锁宽高比",
  "Unlink dimensions": "取消锁定尺寸比例",
  Untitled: "未命名",
  "Unable to load Preset Skills": "无法加载预设 Skill",
  "Update wiki": "更新 Wiki",
  "Upload document": "上传文档",
  View: "查看",
  "View cover": "查看封面",
  "Add document": "添加文档",
  "Add file": "添加文件",
  Uploading: "上传中",
  "Uploading...": "上传中...",
  Wiki: "Wiki",
  "Wiki updates": "文档库更新",
  "Writing mode": "写作模式",
  Create: "创作",
  "New document": "新建文档",
  Revise: "修订",
  Distill: "提炼",
  "Document to revise": "要修订的文档",
  "Select a My Document": "选择我的文档",
  "Select a document from My Documents": "从我的文档中选择一篇文档",
  "Writing instructions": "写作要求",
  "Describe what the agent should write": "描述希望 Agent 写作或修订的内容",
  "New document instructions": "新建文档要求",
  "Revision instructions": "修订要求",
  "Describe what the agent should write in the new document":
    "描述希望 Agent 写作的内容",
  "Describe how the agent should revise this document":
    "描述希望 Agent 如何修订这篇文档",
  "Writing Skills": "Writing Skills",
  "Select one": "选择一个",
  "Select one or more": "选择一个或多个",
  "Multiple Skills generate multiple articles": "多选 Skill 会生成多篇文章",
  "Manage Skill": "管理 Skill",
  "Select Writing Skill": "选择 Writing Skill",
  "Built-in": "内置",
  "Self-built": "自建",
  "Writing Skill actions": "Writing Skill 操作",
  "Skill files": "Skill 文件",
  "Delete Writing Skill?": "删除 Writing Skill？",
  "After deletion, this Writing Skill can no longer be used.":
    "删除后将无法继续使用这个 Writing Skill。",
  "No Writing Skills available": "没有可用的 Writing Skill",
  "No Distillation Skills available": "没有可用的提炼 Skill",
  "Select a Distillation Skill": "请选择一个提炼 Skill",
  "Select at least one Writing Skill": "请至少选择一个 Writing Skill",
  "Revision mode supports one Writing Skill":
    "修订模式只能选择一个 Writing Skill",
  "Start writing": "开始写作",
  "Start revision": "开始修订",
  Submitting: "正在提交",
  "Failed to submit writing request": "写作任务提交失败",
  "Turn selected articles into a Writing Skill":
    "将已选文章提炼为 Writing Skill",
  "The Agent will extract reusable voice, structure, pacing, and techniques from all selected documents in My Documents.":
    "Agent 会从我的文档中所有已选文档里，提炼可复用的语气、结构、节奏和写作技巧。",
  "Selected articles": "已选文章",
  "Selected references": "已选参考资料",
  "Selected reference documents": "已选参考文档",
  "No reference documents selected": "尚未选择参考文档",
  "No structured Wiki documents yet": "还没有结构化 Wiki 文档",
  "Select at least one reference document": "请至少选择一份参考资料",
  "Select at least one document from My Documents": "请至少选择一篇文档",
  "Some selected documents are not ready. Wait for processing or deselect them.":
    "部分已选文档尚未就绪，请等待处理完成或取消选择。",
  "Writing Skill name": "Writing Skill 名称",
  "distillation in progress": "进行中提炼",
  "distillations in progress": "进行中提炼",
  "distillation waiting": "等待中提炼",
  "distillations waiting": "等待中提炼",
  "distillation error": "异常提炼",
  "distillation errors": "异常提炼",
  "Pending creation": "待创作",
  "Pending revision": "待修订",
  "In progress": "进行中",
  "Name this reusable writing method": "为这套可复用的写作方法命名",
  "Start distillation": "开始提炼",
  Distilling: "提炼中",
  "Failed to submit distillation request": "提炼任务提交失败",
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
