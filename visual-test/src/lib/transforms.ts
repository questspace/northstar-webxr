import * as THREE from 'three'
import { useCalibrationProfilesStore } from '../stores/calibrationProfiles'

/**
 * Convert Leap Motion coordinates (mm) to head-relative scene coordinates (meters).
 * Leap on NorthStar points downward from the brow:
 *   Leap X → -Scene X (mirrored for handedness)
 *   Leap Y (up from device) → +Scene Z (forward from face)
 *   Leap Z (away from device) → -Scene Y (down from eyes)
 */
export const leapToScene = (pos: [number, number, number]): THREE.Vector3 => {
  const settings = useCalibrationProfilesStore.getState().settings
  const scale = 0.001 // mm → meters
  const base = new THREE.Vector3(
    -pos[0] * scale,
    -pos[2] * scale + 0.1,   // Leap Z → head Y (with offset for eye height)
    pos[1] * scale + 0.3,    // Leap Y → head +Z (in front of face)
  )
  return new THREE.Vector3(
    base.x * settings.handScaleX + settings.handOffsetX,
    base.y * settings.handScaleY + settings.handOffsetY,
    base.z * settings.handScaleZ + settings.handOffsetZ,
  )
}

/**
 * Convert XR50 pose to scene-space position.
 * XR50 outputs meters; we add a default eye height of 1.6m.
 */
export const xr50ToScene = (
  x: number, y: number, z: number,
  scale = 1.0,
): THREE.Vector3 => {
  const settings = useCalibrationProfilesStore.getState().settings
  return new THREE.Vector3(
    (x * scale) * settings.headScaleX + settings.headOffsetX,
    ((y * scale) + 1.6) * settings.headScaleY + settings.headOffsetY,
    (z * scale) * settings.headScaleZ + settings.headOffsetZ,
  )
}

/**
 * Convert XR50 Euler angles (degrees) to a Three.js Euler (radians, YXZ order).
 */
export const xr50ToEuler = (
  roll: number, pitch: number, yaw: number,
): THREE.Euler => {
  const settings = useCalibrationProfilesStore.getState().settings
  const rollAdj = roll * settings.headRollScale + settings.headRollOffset
  const pitchAdj = pitch * settings.headPitchScale + settings.headPitchOffset
  const yawAdj = yaw * settings.headYawScale + settings.headYawOffset
  return new THREE.Euler(
    THREE.MathUtils.degToRad(pitchAdj),
    THREE.MathUtils.degToRad(yawAdj),
    THREE.MathUtils.degToRad(rollAdj),
    'YXZ',
  )
}