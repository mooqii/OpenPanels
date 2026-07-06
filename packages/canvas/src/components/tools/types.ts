import type React from "react"

/**
 * Individual tool configuration
 */
export interface ToolConfigItem {
  icon: React.ReactNode
  id: string
  label: string
  shortcut?: string
}

/**
 * Tool group configuration with multiple tools
 */
export interface ToolGroupConfig {
  group: string
  tools: ToolConfigItem[]
}

/**
 * Toolbar configuration item - can be either a group or individual tool
 */
export type ToolbarConfig = ToolGroupConfig | ToolConfigItem

/**
 * Toolbar component props
 */
export interface ToolbarProps {
  children?: React.ReactNode
}

/**
 * Type guard to check if a config item is a group
 */
export function isToolGroup(config: ToolbarConfig): config is ToolGroupConfig {
  return "group" in config && "tools" in config
}

/**
 * Extract all tool config items from a toolbar config (flattens groups)
 */
export function getAllToolConfigItems(
  config: ToolbarConfig[]
): ToolConfigItem[] {
  const items: ToolConfigItem[] = []
  for (const item of config) {
    if (isToolGroup(item)) {
      items.push(...item.tools)
    } else {
      items.push(item)
    }
  }
  return items
}

/**
 * Create a map of shortcut keys to tool IDs from toolbar config
 * Stores shortcuts exactly as specified (case-sensitive)
 * Only stores lowercase version for shortcuts that are already lowercase
 * (for case-insensitive fallback when user types lowercase)
 */
export function getShortcutMap(config: ToolbarConfig[]): Map<string, string> {
  const map = new Map<string, string>()
  const items = getAllToolConfigItems(config)
  for (const item of items) {
    if (item.shortcut) {
      // Store exact case (case-sensitive)
      map.set(item.shortcut, item.id)
      // Only store lowercase version if shortcut is already lowercase
      // This allows case-insensitive matching for lowercase shortcuts
      // but preserves case-sensitivity for uppercase shortcuts
      if (item.shortcut === item.shortcut.toLowerCase()) {
        // Shortcut is already lowercase, so it's case-insensitive
        // (no need to store again, it's the same)
      }
    }
  }
  return map
}
