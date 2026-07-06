// =============================================================================
// Rotation Zone Constants
// =============================================================================

// Rotation zone configuration
export const ROTATION_ZONE_INNER_OFFSET = 8 // pixels from corner
export const ROTATION_ZONE_OUTER_OFFSET = 16 // pixels outward from corner
export const ROTATION_SECTOR_HALF_ANGLE = (45 * Math.PI) / 180

// =============================================================================
// Rotation Utility Functions
// =============================================================================

/**
 * Create rotation cursor SVG
 */
export function createRotationCursor(angleDegrees = 0): string {
  const svg = `<svg height='32' width='32' viewBox='0 0 32 32' xmlns='http://www.w3.org/2000/svg' style='color: black;'>
     <defs>
       <filter id='shadow' y='-40%' x='-40%' width='180px' height='180%' color-interpolation-filters='sRGB'>
         <feDropShadow dx='0.16879857377580254' dy='-1.4041036434292358' stdDeviation='1.2' flood-opacity='.3'/>
       </filter>
     </defs>
     <g fill='none' transform='rotate(${angleDegrees} 16 16)' filter='url(%23shadow)'>
       <g>
         <path d='M20.8514 9.7725L23.5 12.4212M23.5 12.4212L20.8514 15.0698M23.5 12.4212C23.5 12.4212 21.6089 12.0427 19.7162 12.4212C17.8235 12.7997 15.9443 13.7046 14.4189 15.4482C12.8936 17.1919 12.3739 19.1127 12.1486 20.7455C11.9234 22.3783 12.1486 24.5293 12.1486 24.5293M12.1486 24.5293L14.7973 21.8806M12.1486 24.5293L9.5 21.8806' stroke='white' stroke-width='4' stroke-linecap='round' stroke-linejoin='round'/>
       </g>
       <path d='M20.8514 9.7725L23.5 12.4212M23.5 12.4212L20.8514 15.0698M23.5 12.4212C23.5 12.4212 21.6089 12.0427 19.7162 12.4212C17.8235 12.7997 15.9443 13.7046 14.4189 15.4482C12.8936 17.1919 12.3739 19.1127 12.1486 20.7455C11.9234 22.3783 12.1486 24.5293 12.1486 24.5293M12.1486 24.5293L14.7973 21.8806M12.1486 24.5293L9.5 21.8806' stroke='black' stroke-linecap='round' stroke-linejoin='round' stroke-width="1.5"/>
  </g>
</svg>`

  const encodedSvg = encodeURIComponent(svg)
  return `url("data:image/svg+xml;charset=utf-8,${encodedSvg}") 12 12, pointer`
}

/**
 * Get rotation cursor angle for a given corner index and shape rotation
 */
export function getRotationCursorAngle(
  cornerIndex: number,
  rotation: number
): number {
  const baseAngles = [0, 90, 180, -90]
  return baseAngles[cornerIndex] + rotation
}

/**
 * Get outward angle for each corner (for rotation zone detection)
 */
export function getCornerOutwardAngle(
  cornerIndex: number,
  shapeRotation: number
): number {
  const baseAngles = [
    (-135 * Math.PI) / 180, // top-left
    (-45 * Math.PI) / 180, // top-right
    (45 * Math.PI) / 180, // bottom-right
    (135 * Math.PI) / 180, // bottom-left
  ]
  return baseAngles[cornerIndex] + (shapeRotation * Math.PI) / 180
}

/**
 * Check if point is in a sector (for rotation zones)
 */
export function isPointInSector(
  point: { x: number; y: number },
  center: { x: number; y: number },
  direction: number,
  halfAngle: number,
  minRadius: number,
  maxRadius: number
): boolean {
  const dx = point.x - center.x
  const dy = point.y - center.y
  const distance = Math.sqrt(dx * dx + dy * dy)

  if (distance < minRadius || distance > maxRadius) {
    return false
  }

  const angleToPoint = Math.atan2(dy, dx)
  let angleDiff = angleToPoint - direction
  while (angleDiff > Math.PI) angleDiff -= 2 * Math.PI
  while (angleDiff < -Math.PI) angleDiff += 2 * Math.PI

  return Math.abs(angleDiff) <= halfAngle
}

/**
 * Rotate a point around a center point by a given angle
 */
export function rotatePointAroundCenter(
  point: { x: number; y: number },
  center: { x: number; y: number },
  angleRad: number
): { x: number; y: number } {
  const cos = Math.cos(angleRad)
  const sin = Math.sin(angleRad)
  const dx = point.x - center.x
  const dy = point.y - center.y
  return {
    x: center.x + dx * cos - dy * sin,
    y: center.y + dx * sin + dy * cos,
  }
}
