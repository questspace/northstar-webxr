import * as THREE from 'three'
import { useCalibrationProfilesStore } from '../stores/calibrationProfiles'
import type { NorthStarCalibration } from './calibration'
import type { HeadPose, Quat, Vec3 } from '../stores/tracking'

/**
 * Frame conventions, checked against live XR50 data and on the headset
 * (see PROTOCOL.md):
 *  - XR50 device frame: X right, Y down, Z forward (camera convention).
 *  - Its world frame sits 180° about Z from the device when the headset is level
 *    (rest quaternion ≈ [0.05, 0.22, 0.97, -0.12]): X left, Y up, Z forward.
 *    (The mirror reading — world Y down, device X left — fits the rest pose too,
 *    but made the world turn with the head when worn.)
 *  - Three.js world: X right, Y up, Z back; a camera looks down -Z.
 */
const XR50_WORLD_TO_THREE = new THREE.Quaternion(0, 1, 0, 0) // 180° about Y: (x, y, z) → (-x, y, -z)
const XR50_DEVICE_TO_CAMERA = new THREE.Quaternion(1, 0, 0, 0) // 180° about X

/** Height of the SLAM origin above the virtual floor. */
const EYE_HEIGHT = 1.6

const degToQuat = (pitch: number, yaw: number, roll: number) =>
  new THREE.Quaternion().setFromEuler(new THREE.Euler(
    THREE.MathUtils.degToRad(pitch),
    THREE.MathUtils.degToRad(yaw),
    THREE.MathUtils.degToRad(roll),
    'YXZ',
  ))

/** Raw XR50 pose → tracker pose in the Three.js world frame. */
export const xr50ToThree = (position: Vec3, quaternion: Quat) => {
  const q = new THREE.Quaternion()
    .copy(XR50_WORLD_TO_THREE)
    .multiply(new THREE.Quaternion().fromArray(quaternion))
    .multiply(XR50_DEVICE_TO_CAMERA)
  return {
    position: [-position[0], position[1], -position[2]] as Vec3,
    quaternion: q.toArray() as Quat,
  }
}

/**
 * Tracker pose → camera rig pose: applies the tracker→head calibration, the
 * user's head offsets and lifts the SLAM origin to eye height.
 */
export const headPoseToRig = (pose: HeadPose, calibration: NorthStarCalibration) => {
  const s = useCalibrationProfilesStore.getState().settings
  const tracker = new THREE.Quaternion().fromArray(pose.quaternion)
  const quaternion = tracker.clone()
    .multiply(calibration.trackerRotation)
    .multiply(degToQuat(s.headPitchOffset, s.headYawOffset, s.headRollOffset))
  const position = calibration.trackerTranslation.clone()
    .applyQuaternion(tracker)
    .add(new THREE.Vector3().fromArray(pose.position))
    .add(new THREE.Vector3(s.headOffsetX, s.headOffsetY + EYE_HEIGHT, s.headOffsetZ))
  return { position, quaternion }
}

/**
 * Leap sensor frame → head frame for the HMD mount, measured on this NorthStar:
 * hand right → Leap -X, hand up → Leap -Z, hand forward (away from the face) → Leap +Y.
 * This is Ultraleap's standard HMD basis; the calibrated sensor tilt goes on top.
 */
const LEAP_HMD_MOUNT = new THREE.Quaternion().setFromRotationMatrix(
  new THREE.Matrix4().set(
    -1, 0, 0, 0,
    0, 0, -1, 0,
    0, -1, 0, 0,
    0, 0, 0, 1,
  ),
)

/** Builds the Leap → head frame transform for the current settings: points (mm → m) and directions. */
export const leapToHead = (calibration: NorthStarCalibration) => {
  const s = useCalibrationProfilesStore.getState().settings
  const rotation = degToQuat(s.handPitchOffset, s.handYawOffset, s.handRollOffset)
    .multiply(calibration.leapRotation)
    .multiply(LEAP_HMD_MOUNT)
  const offset = calibration.leapTranslation.clone()
    .add(new THREE.Vector3(s.handOffsetX, s.handOffsetY, s.handOffsetZ))
  return {
    point: (posMm: Vec3) =>
      new THREE.Vector3().fromArray(posMm).multiplyScalar(0.001).applyQuaternion(rotation).add(offset),
    direction: (dir: Vec3) => new THREE.Vector3().fromArray(dir).applyQuaternion(rotation),
  }
}

/** Inverse of `leapToHead`: head-frame points (m) and directions → Leap sensor frame (mm). */
export const headToLeap = (calibration: NorthStarCalibration) => {
  const s = useCalibrationProfilesStore.getState().settings
  const rotation = degToQuat(s.handPitchOffset, s.handYawOffset, s.handRollOffset)
    .multiply(calibration.leapRotation)
    .multiply(LEAP_HMD_MOUNT)
    .invert()
  const offset = calibration.leapTranslation.clone()
    .add(new THREE.Vector3(s.handOffsetX, s.handOffsetY, s.handOffsetZ))
  return {
    point: (p: THREE.Vector3) => p.clone().sub(offset).applyQuaternion(rotation).multiplyScalar(1000),
    direction: (d: THREE.Vector3) => d.clone().applyQuaternion(rotation),
  }
}
