/**
 * Gradient preset definitions for quick selection in the Fill panel.
 */

import type { LinearGradientFill } from "../types/shapes"

export interface GradientPreset {
  fill: LinearGradientFill
  id: string
  name: string
}

/**
 * 12 preset gradients matching the design mockup.
 * Arranged in 2 rows of 6 for the UI grid.
 */
export const GRADIENT_PRESETS: GradientPreset[] = [
  // Row 1 - Soft pastels
  {
    id: "preset-sky-mist",
    name: "Sky Mist",
    fill: {
      type: "linear-gradient",
      colorStops: [
        { offset: 0, color: "rgba(224, 242, 254, 1)" },
        { offset: 1, color: "rgba(186, 230, 253, 1)" },
      ],
      rotation: 135,
    },
  },
  {
    id: "preset-mint-fresh",
    name: "Mint Fresh",
    fill: {
      type: "linear-gradient",
      colorStops: [
        { offset: 0, color: "rgba(209, 250, 229, 1)" },
        { offset: 1, color: "rgba(167, 243, 208, 1)" },
      ],
      rotation: 135,
    },
  },
  {
    id: "preset-ocean-breeze",
    name: "Ocean Breeze",
    fill: {
      type: "linear-gradient",
      colorStops: [
        { offset: 0, color: "rgba(34, 197, 94, 1)" },
        { offset: 1, color: "rgba(56, 189, 248, 1)" },
      ],
      rotation: 90,
    },
  },
  {
    id: "preset-emerald-sea",
    name: "Emerald Sea",
    fill: {
      type: "linear-gradient",
      colorStops: [
        { offset: 0, color: "rgba(74, 222, 128, 1)" },
        { offset: 1, color: "rgba(59, 130, 246, 1)" },
      ],
      rotation: 135,
    },
  },
  {
    id: "preset-lavender-dream",
    name: "Lavender Dream",
    fill: {
      type: "linear-gradient",
      colorStops: [
        { offset: 0, color: "rgba(196, 181, 253, 1)" },
        { offset: 1, color: "rgba(167, 139, 250, 1)" },
      ],
      rotation: 135,
    },
  },
  {
    id: "preset-silver-mist",
    name: "Silver Mist",
    fill: {
      type: "linear-gradient",
      colorStops: [
        { offset: 0, color: "rgba(243, 244, 246, 1)" },
        { offset: 1, color: "rgba(156, 163, 175, 1)" },
      ],
      rotation: 135,
    },
  },

  // Row 2 - Vibrant gradients
  {
    id: "preset-peach-glow",
    name: "Peach Glow",
    fill: {
      type: "linear-gradient",
      colorStops: [
        { offset: 0, color: "rgba(254, 215, 170, 1)" },
        { offset: 1, color: "rgba(253, 186, 116, 1)" },
      ],
      rotation: 135,
    },
  },
  {
    id: "preset-rose-petal",
    name: "Rose Petal",
    fill: {
      type: "linear-gradient",
      colorStops: [
        { offset: 0, color: "rgba(251, 207, 232, 1)" },
        { offset: 1, color: "rgba(244, 114, 182, 1)" },
      ],
      rotation: 135,
    },
  },
  {
    id: "preset-cherry-blossom",
    name: "Cherry Blossom",
    fill: {
      type: "linear-gradient",
      colorStops: [
        { offset: 0, color: "rgba(253, 164, 175, 1)" },
        { offset: 1, color: "rgba(244, 63, 94, 1)" },
      ],
      rotation: 135,
    },
  },
  {
    id: "preset-sunset-glow",
    name: "Sunset Glow",
    fill: {
      type: "linear-gradient",
      colorStops: [
        { offset: 0, color: "rgba(253, 224, 71, 1)" },
        { offset: 1, color: "rgba(251, 146, 60, 1)" },
      ],
      rotation: 135,
    },
  },
  {
    id: "preset-aurora",
    name: "Aurora",
    fill: {
      type: "linear-gradient",
      colorStops: [
        { offset: 0, color: "rgba(165, 243, 252, 1)" },
        { offset: 1, color: "rgba(192, 132, 252, 1)" },
      ],
      rotation: 135,
    },
  },
  {
    id: "preset-candy-floss",
    name: "Candy Floss",
    fill: {
      type: "linear-gradient",
      colorStops: [
        { offset: 0, color: "rgba(251, 207, 232, 1)" },
        { offset: 1, color: "rgba(253, 230, 138, 1)" },
      ],
      rotation: 135,
    },
  },
]

/**
 * Get a preset by ID
 */
export function getGradientPreset(id: string): GradientPreset | undefined {
  return GRADIENT_PRESETS.find((preset) => preset.id === id)
}
