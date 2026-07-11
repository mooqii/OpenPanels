import { Button, Popover } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { Info } from "lucide-react"
import type { ReactNode, WheelEvent } from "react"
import { useMemo, useState } from "react"
import type { Asset } from "../types/assets"

type ParsedImageMetadata = {
  dimensions: {
    height: number | null
    width: number | null
  }
  generation: {
    model: string | null
    prompt: string | null
    references: string[]
  }
  mimeType: string | null
  name: string | null
}

export function parseImageMetadata(asset: Asset): ParsedImageMetadata {
  const generation = parseGenerationMetadata(asset.meta)
  return {
    dimensions: {
      width: "w" in asset.props ? asset.props.w : null,
      height: "h" in asset.props ? asset.props.h : null,
    },
    generation,
    mimeType: "mimeType" in asset.props ? asset.props.mimeType : null,
    name: "name" in asset.props ? asset.props.name : null,
  }
}

function parseGenerationMetadata(
  meta: unknown
): ParsedImageMetadata["generation"] {
  const generateOptions =
    meta && typeof meta === "object" && !Array.isArray(meta)
      ? (meta as { generateOptions?: unknown }).generateOptions
      : null
  const options =
    generateOptions &&
    typeof generateOptions === "object" &&
    !Array.isArray(generateOptions)
      ? (generateOptions as Record<string, unknown>)
      : {}
  return {
    model: typeof options.model === "string" ? options.model : null,
    prompt: typeof options.prompt === "string" ? options.prompt : null,
    references: parseReferenceImages(options.referenceImages),
  }
}

function parseReferenceImages(value: unknown): string[] {
  if (!Array.isArray(value)) return []
  return value
    .map((item) => {
      if (typeof item === "string") return item
      if (!(item && typeof item === "object" && !Array.isArray(item))) {
        return null
      }
      const record = item as Record<string, unknown>
      for (const key of ["path", "assetRef", "url", "shapeId", "id"]) {
        const value = record[key]
        if (typeof value === "string" && value.trim()) return value
      }
      return null
    })
    .filter((item): item is string => Boolean(item))
}

function stopWheelPropagation(event: WheelEvent<HTMLElement>) {
  event.stopPropagation()
}

function MetadataRow({ label, value }: { label: ReactNode; value: ReactNode }) {
  if (!value) return null

  return (
    <div className="grid grid-cols-[auto_1fr] items-start gap-x-3 gap-y-1 text-sm">
      <dt className="text-text-tertiary">{label}</dt>
      <dd className="min-w-0 whitespace-pre-wrap break-words text-text-primary">
        {value}
      </dd>
    </div>
  )
}

export function ImageMetadataTooltipContent({ asset }: { asset: Asset }) {
  const { t } = useLingui()
  const parsed = useMemo(() => parseImageMetadata(asset), [asset])
  const dimensionsLabel =
    parsed.dimensions.width && parsed.dimensions.height
      ? `${parsed.dimensions.width} × ${parsed.dimensions.height}`
      : null

  return (
    <div
      className="w-72 space-y-3 p-4"
      data-testid="canvas-image-metadata-content"
      onWheel={stopWheelPropagation}
    >
      <dl className="space-y-2">
        <MetadataRow label={t`Name`} value={parsed.name} />
        <MetadataRow label={t`Dimensions`} value={dimensionsLabel} />
        <MetadataRow label={t`Type`} value={parsed.mimeType} />
        <MetadataRow label={t`Prompt`} value={parsed.generation.prompt} />
        <MetadataRow label={t`Model`} value={parsed.generation.model} />
        <MetadataRow
          label={t`References`}
          value={
            parsed.generation.references.length
              ? parsed.generation.references.join("\n")
              : null
          }
        />
      </dl>
    </div>
  )
}

export function ImageMetadataItem({ asset }: { asset: Asset }) {
  const { t } = useLingui()
  const [isOpen, setIsOpen] = useState(false)

  return (
    <Popover isOpen={isOpen} onOpenChange={setIsOpen}>
      <Popover.Trigger>
        <Button
          aria-label={t`Image info`}
          className="cursor-pointer"
          data-testid="canvas-image-metadata-trigger"
          isIconOnly
          variant="ghost"
        >
          <Info size={16} strokeWidth={1.75} />
        </Button>
      </Popover.Trigger>
      <Popover.Content
        className="overflow-hidden rounded-xl border border-border-default bg-bg-base p-0 shadow-lg"
        offset={10}
        placement="bottom"
        shouldFlip={false}
      >
        <Popover.Dialog className="p-0" onWheel={stopWheelPropagation}>
          <ImageMetadataTooltipContent asset={asset} />
        </Popover.Dialog>
      </Popover.Content>
    </Popover>
  )
}
