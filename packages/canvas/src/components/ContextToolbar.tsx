import { memo } from "react"
import type { Editor } from "../editor"
import type { Transformer } from "../shapes/Transformer"
import type { GeoShape, Shape, TextShape } from "../types/shapes"
import type { ToolConfig } from "../types/tools"
import { CommandItem } from "./toolbar-items/CommandItem"
import { CornerRadiusItem } from "./toolbar-items/CornerRadiusItem"
import { CropItem } from "./toolbar-items/CropItem"
import { DimensionsItem } from "./toolbar-items/DimensionsItem"
import { DownloadItem } from "./toolbar-items/DownloadItem"
import { FillItem } from "./toolbar-items/FillItem"
import { FontFamilySelect } from "./toolbar-items/FontFamilySelect"
import { FontSizeSelect } from "./toolbar-items/FontSizeSelect"
import { FontStyleSelect } from "./toolbar-items/FontStyleSelect"
import { GroupItem, UngroupItem } from "./toolbar-items/GroupItem"
import { ImageInfoItem } from "./toolbar-items/ImageInfoItem"
import { RasterizeItem } from "./toolbar-items/RasterizeItem"
import { ShadowItem } from "./toolbar-items/ShadowItem"
import { StrokeItem } from "./toolbar-items/StrokeItem"
import { TextAlignSelect } from "./toolbar-items/TextAlignSelect"
import { TextColorItem } from "./toolbar-items/TextColorItem"

interface ContextToolbarProps {
  editor: Editor
  onWheel?: (e: React.WheelEvent) => void
  ref: React.RefObject<HTMLDivElement | null>
  selectedShape: Shape | null
  tools: ToolConfig[]
  transformerRef: React.RefObject<Transformer | null>
}

export const ContextToolbar = memo(function ContextToolbar({
  ref,
  editor,
  tools,
  selectedShape,
  transformerRef,
  onWheel,
}: ContextToolbarProps) {
  // No tools to show
  if (tools.length === 0) {
    return (
      <div
        className="shape-toolbar-container pointer-events-none absolute flex justify-center"
        ref={ref}
        style={{
          left: "0px",
          top: "0px",
          width: "0px",
          height: "0px",
          opacity: 0,
        }}
      />
    )
  }

  const renderToolItem = (tool: ToolConfig, index: number) => {
    switch (tool.type) {
      case "divider":
        return (
          <div className="mx-1 h-6 w-px bg-border" key={`divider-${index}`} />
        )
      case "group":
        return <GroupItem editor={editor} key={tool.type} />
      case "ungroup":
        return <UngroupItem editor={editor} key={tool.type} />
      case "rasterize":
        return (
          <RasterizeItem
            editor={editor}
            key={tool.type}
            transformerRef={transformerRef}
          />
        )
      case "fill":
        return (
          <FillItem
            editor={editor}
            key={tool.type}
            shape={selectedShape as GeoShape}
          />
        )
      case "stroke":
        return (
          <StrokeItem
            editor={editor}
            key={tool.type}
            shape={selectedShape as GeoShape}
          />
        )
      case "shadow":
        return (
          <ShadowItem
            editor={editor}
            key={tool.type}
            shape={selectedShape as GeoShape}
          />
        )
      case "corner-radius":
        return (
          <CornerRadiusItem
            editor={editor}
            key={tool.type}
            shape={selectedShape as GeoShape}
          />
        )
      case "dimensions":
        return (
          <DimensionsItem
            editor={editor}
            key={tool.type}
            shape={selectedShape as GeoShape}
          />
        )
      case "download":
        return (
          <DownloadItem
            editor={editor}
            key={tool.type}
            shape={selectedShape}
            transformerRef={transformerRef}
          />
        )
      case "info":
        return (
          <ImageInfoItem
            editor={editor}
            key={tool.type}
            shape={selectedShape}
          />
        )
      case "command":
        return (
          <CommandItem
            editor={editor}
            key={`${tool.type}-${tool.command}`}
            tool={tool}
            transformerRef={transformerRef}
          />
        )
      case "text-color":
        return (
          <TextColorItem
            editor={editor}
            key={tool.type}
            shape={selectedShape as TextShape}
          />
        )
      case "text-font":
        return (
          <FontFamilySelect
            editor={editor}
            key={tool.type}
            shape={selectedShape as TextShape}
          />
        )
      case "text-style":
        return (
          <FontStyleSelect
            editor={editor}
            key={tool.type}
            shape={selectedShape as TextShape}
          />
        )
      case "text-size":
        return (
          <FontSizeSelect
            editor={editor}
            key={tool.type}
            shape={selectedShape as TextShape}
          />
        )
      case "text-align":
        return (
          <TextAlignSelect
            editor={editor}
            key={tool.type}
            shape={selectedShape as TextShape}
          />
        )
      case "crop":
        return <CropItem key={tool.type} shape={selectedShape} />
      default:
        return null
    }
  }

  return (
    <div
      className="shape-toolbar-container pointer-events-none absolute flex justify-center"
      ref={ref}
      style={{
        left: "0px",
        top: "0px",
        width: "0px",
        height: "0px",
        opacity: 0,
      }}
    >
      <div
        className="shape-toolbar pointer-events-auto flex h-11 -translate-y-14 items-center justify-center gap-2 rounded-full bg-canvas-toolbar px-2 shadow backdrop-blur-lg"
        onWheel={onWheel}
      >
        {tools.map((tool, index) => renderToolItem(tool, index))}
      </div>
    </div>
  )
})
