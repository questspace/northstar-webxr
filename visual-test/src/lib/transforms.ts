import * as THREE from 'three'

/**
 * Convert Leap Motion coordinates (mm) to head-relative scene coordinates (meters).
 * Leap on NorthStar points downward from the brow:
 *   Leap X → -Scene X (mirrored for handedness)
 *   Leap Y (up from device) → -Scene Z (forward from face)
 *   Leap Z (away from device) → -Scene Y (down from eyes)
 */
export const leapToScene = (pos: [number, number, number]): THREE.Vector3 => {
  const scale = 0.001 // mm → meters
  return new THREE.Vector3(
    -pos[0] * scale,
    -pos[2] * scale + 0.1,   // Leap Z → head Y (with offset for eye height)
    -pos[1] * scale + 0.3,   // Leap Y → head -Z (in front of face)
  )
}

/**
 * Convert XR50 pose to scene-space position.
 * XR50 outputs meters; we add a default eye height of 1.6m.
 */
export const xr50ToScene = (
  x: number, y: number, z: number,
  scale = 1.0,
): THREE.Vector3 =>
  new THREE.Vector3(x * scale, y * scale + 1.6, -z * scale)

/**
 * Convert XR50 Euler angles (degrees) to a Three.js Euler (radians, YXZ order).
 */
export const xr50ToEuler = (
  roll: number, pitch: number, yaw: number,
): THREE.Euler =>
  new THREE.Euler(
    THREE.MathUtils.degToRad(pitch),
    THREE.MathUtils.degToRad(yaw),
    THREE.MathUtils.degToRad(roll),
    'YXZ',
  )
