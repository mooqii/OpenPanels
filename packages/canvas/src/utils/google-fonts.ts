/**
 * Curated list of Google Fonts with their available weights
 */
export interface GoogleFont {
  family: string
  weights: string[]
}

export const GOOGLE_FONTS: GoogleFont[] = [
  { family: "Roboto", weights: ["100", "300", "400", "500", "700", "900"] },
  { family: "Open Sans", weights: ["300", "400", "600", "700", "800"] },
  { family: "Shrikhand", weights: ["400"] },
  { family: "Lato", weights: ["300", "400", "700", "900"] },
  {
    family: "Montserrat",
    weights: ["300", "400", "500", "600", "700", "800", "900"],
  },
  { family: "Oswald", weights: ["300", "400", "500", "600", "700"] },
  {
    family: "Raleway",
    weights: ["300", "400", "500", "600", "700", "800", "900"],
  },
  {
    family: "Poppins",
    weights: ["300", "400", "500", "600", "700", "800", "900"],
  },
  {
    family: "Playfair Display",
    weights: ["400", "500", "600", "700", "800", "900"],
  },
  { family: "Merriweather", weights: ["300", "400", "700", "900"] },
  { family: "Source Sans Pro", weights: ["300", "400", "600", "700", "900"] },
  {
    family: "Nunito",
    weights: ["300", "400", "500", "600", "700", "800", "900"],
  },
  {
    family: "Inter",
    weights: ["300", "400", "500", "600", "700", "800", "900"],
  },
  {
    family: "Work Sans",
    weights: ["300", "400", "500", "600", "700", "800", "900"],
  },
  {
    family: "Fira Sans",
    weights: ["300", "400", "500", "600", "700", "800", "900"],
  },
]

/**
 * System fonts that don't need loading
 */
export const SYSTEM_FONTS = [
  "Arial",
  "Times New Roman",
  "Courier New",
  "Georgia",
  "Verdana",
  "Comic Sans MS",
  "Impact",
  "Trebuchet MS",
  "Helvetica",
  "Tahoma",
  "Palatino",
  "Garamond",
]

/**
 * Check if a font is a Google Font
 */
export function isGoogleFont(family: string): boolean {
  return GOOGLE_FONTS.some((font) => font.family === family)
}

/**
 * Get available weights for a font family
 */
export function getFontWeights(family: string): string[] {
  const googleFont = GOOGLE_FONTS.find((font) => font.family === family)
  if (googleFont) {
    return googleFont.weights
  }
  // Default weights for system fonts
  return ["300", "400", "500", "600", "700", "800", "900"]
}

/**
 * Check if a font is loaded by testing with a canvas
 */
export function isFontLoaded(family: string): boolean {
  if (!isGoogleFont(family)) {
    // System fonts are always "loaded"
    return true
  }

  // Check if font is in document.fonts
  if (document.fonts?.check) {
    // Try checking with a common weight
    return document.fonts.check(`12px "${family}"`)
  }

  // Fallback: try to measure text with the font
  const canvas = document.createElement("canvas")
  const context = canvas.getContext("2d")
  if (!context) return false

  const testString = "mmmmmmmmmmlli"
  const baselineFont = "monospace"
  const testFont = `12px "${family}"`

  context.font = baselineFont
  const baselineWidth = context.measureText(testString).width

  context.font = testFont
  const testWidth = context.measureText(testString).width

  // If widths are different, font is likely loaded
  return Math.abs(baselineWidth - testWidth) > 0.1
}

/**
 * Load a Google Font dynamically
 */
export function loadGoogleFont(family: string, weight = "400"): Promise<void> {
  if (!isGoogleFont(family)) {
    return Promise.resolve()
  }

  // Check if already loaded
  if (isFontLoaded(family)) {
    return Promise.resolve()
  }

  // Create link element to load font
  const linkId = `google-font-${family.toLowerCase().replace(/\s+/g, "-")}`
  const existingLink = document.getElementById(linkId)

  if (existingLink) {
    // Font is already being loaded or loaded
    return Promise.resolve()
  }

  // Build Google Fonts URL
  const fontFamilyParam = family.replace(/\s+/g, "+")
  const weightsParam = weight
  const url = `https://fonts.googleapis.com/css2?family=${fontFamilyParam}:wght@${weightsParam}&display=swap`

  // Create and append link
  const link = document.createElement("link")
  link.id = linkId
  link.rel = "stylesheet"
  link.href = url
  document.head.appendChild(link)

  // Wait for font to load
  return new Promise((resolve) => {
    if (document.fonts?.ready) {
      document.fonts.ready.then(() => {
        // Additional check to ensure font is loaded
        const checkInterval = setInterval(() => {
          if (isFontLoaded(family)) {
            clearInterval(checkInterval)
            resolve()
          }
        }, 100)

        // Timeout after 5 seconds
        setTimeout(() => {
          clearInterval(checkInterval)
          resolve()
        }, 5000)
      })
    } else {
      // Fallback: wait a bit and resolve
      setTimeout(resolve, 1000)
    }
  })
}

/**
 * Load multiple weights of a Google Font
 */
export function loadGoogleFontWeights(
  family: string,
  weights: string[]
): Promise<void> {
  if (!isGoogleFont(family)) {
    return Promise.resolve()
  }

  // Load all weights
  const uniqueWeights = [...new Set(weights)]
  const fontFamilyParam = family.replace(/\s+/g, "+")
  const weightsParam = uniqueWeights.join(";")
  const url = `https://fonts.googleapis.com/css2?family=${fontFamilyParam}:wght@${weightsParam}&display=swap`

  const linkId = `google-font-${family.toLowerCase().replace(/\s+/g, "-")}`
  const existingLink = document.getElementById(linkId)

  if (existingLink) {
    // Update existing link if needed
    existingLink.setAttribute("href", url)
  } else {
    const link = document.createElement("link")
    link.id = linkId
    link.rel = "stylesheet"
    link.href = url
    document.head.appendChild(link)
  }

  // Wait for fonts to load
  return new Promise((resolve) => {
    if (document.fonts?.ready) {
      document.fonts.ready.then(() => {
        const checkInterval = setInterval(() => {
          if (isFontLoaded(family)) {
            clearInterval(checkInterval)
            resolve()
          }
        }, 100)

        setTimeout(() => {
          clearInterval(checkInterval)
          resolve()
        }, 5000)
      })
    } else {
      setTimeout(resolve, 1000)
    }
  })
}
