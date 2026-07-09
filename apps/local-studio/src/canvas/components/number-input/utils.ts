const TRAILING_ZEROS_REGEX = /\.?0+$/

/**
 * Clamp a number between min and max bounds
 */
export function clampValue(value: number, min?: number, max?: number): number {
  if (min !== undefined && value < min) return min
  if (max !== undefined && value > max) return max
  return value
}

/**
 * Round a number to specified decimal places
 */
export function roundToPrecision(value: number, precision?: number): number {
  if (precision === undefined) return value
  const multiplier = 10 ** precision
  return Math.round(value * multiplier) / multiplier
}

/**
 * Format number for display with auto or specified precision
 */
export function formatNumber(value: number, precision?: number): string {
  if (precision === undefined) {
    // Auto-detect: show decimals only if needed
    return Number.isInteger(value)
      ? String(value)
      : value.toFixed(2).replace(TRAILING_ZEROS_REGEX, "")
  }
  return value.toFixed(precision)
}
