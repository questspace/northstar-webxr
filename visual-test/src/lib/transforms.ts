import * as THREE from 'three'
import { useCalibrationProfilesStore } from '../stores/calibrationProfiles'
import type { NorthStarCalibration } from './calibration'
import type { HeadPose, Quat, Vec3 } from '../stores/tracking'

/**
 * Frame conventions, checked against live XR50 data and on the headset
 * (see PROTOCOL.md):
 *  - XR50 device frame and SLAM world frame: both X right, Y down, Z forward
 *    (camera convention); the world frame is set at start-up, ~180° about Z from
 *    the device when level (rest quaternion ≈ [0.05, 0.22, 0.97, -0.12]).
 *  - Three.js world: X right, Y up, Z back; a camera looks down -Z.
 *  Verified on the head 2026-10-26 with the panel rotated 180° in Windows. The
 *  earlier Y-based constants rolled the view and cancelled inverted motion, which
 *  looked right until the roll was fixed.
 */
const XR50_WORLD_TO_THREE = new THREE.Quaternion(1, 0, 0, 0) // 180° about X: (x, y, z) → (x, -y, -z)
const XR50_DEVICE_TO_CAMERA = new THREE.Quaternion(1, 0, 0, 0) // 180° about X: camera convention → GL camera

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
    position: [position[0], -position[1], -position[2]] as Vec3,
    quaternion: q.toArray() as Quat,
  }
}

/** A detected plane as the bridge sends it: raw XR50 world frame, meters. */
export interface XR50Plane {
  id: string
  /** Unit normal */
  n: Vec3
  /** Signed distance to the world origin along the normal */
  d: number
  /** Border polygon */
  p: Vec3[]
}

/** A plane in the Three.js world the head rig lives in (same origin shift as the rig). */
export interface WorldPlane {
  id: string
  normal: THREE.Vector3
  border: THREE.Vector3[]
}

/** Raw XR50 world point → Three.js world: axis flip, then the rig's world offsets. */
const xr50PointToWorld = ([x, y, z]: Vec3) => {
  const s = useCalibrationProfilesStore.getState().settings
  return new THREE.Vector3(x + s.headOffsetX, -y + s.headOffsetY + EYE_HEIGHT, -z + s.headOffsetZ)
}

/** Raw XR50 world positions (x, y, z, x, y, z, ...) → Three.js world, in place. */
export const xr50PositionsToWorld = (positions: Float32Array) => {
  const s = useCalibrationProfilesStore.getState().settings
  for (let i = 0; i < positions.length; i += 3) {
    positions[i] += s.headOffsetX
    positions[i + 1] = -positions[i + 1] + s.headOffsetY + EYE_HEIGHT
    positions[i + 2] = -positions[i + 2] + s.headOffsetZ
  }
  return positions
}

/** One block of the room mesh (bridge recon source), Three.js world. */
export interface MeshBlock {
  key: string
  positions: Float32Array
  indices: Uint32Array
}

/**
 * Decodes a mesh-block message (xvisio-rs `recon::map` wire format: u32 count, then per
 * block i32x3 key, u32 vertex count, u32 index count, f32 positions, u32 indices).
 * A block without triangles was removed.
 */
export const decodeMeshBlocks = (buffer: ArrayBuffer): MeshBlock[] => {
  const u32 = new Uint32Array(buffer)
  const i32 = new Int32Array(buffer)
  const blocks: MeshBlock[] = []
  let at = 1
  for (let b = 0; b < u32[0]; b++) {
    const key = `${i32[at]},${i32[at + 1]},${i32[at + 2]}`
    const [n, m] = [u32[at + 3], u32[at + 4]]
    at += 5
    const positions = xr50PositionsToWorld(new Float32Array(buffer, at * 4, n * 3).slice())
    at += n * 3
    const indices = new Uint32Array(buffer, at * 4, m).slice()
    at += m
    blocks.push({ key, positions, indices })
  }
  return blocks
}

export const xr50PlaneToWorld = (plane: XR50Plane): WorldPlane => ({
  id: plane.id,
  normal: new THREE.Vector3(plane.n[0], -plane.n[1], -plane.n[2]),
  border: plane.p.map(xr50PointToWorld),
})

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

/**
 * The rig (render) frame is the real head frame rolled 180° about the view axis. The Esky warp
 * is fitted to a panel Windows shows rotated 180°, and the XR50 conversion was tuned on the head
 * against that, so world content is upright while anything measured in the real head frame (the
 * Leap and its calibrated offset) must be rolled into the rig. Found on the head 2026-10-27:
 * hands moved opposite on both screen axes with depth correct.
 */
const HEAD_TO_RIG = new THREE.Quaternion(0, 0, 1, 0)

/** Builds the Leap → head (rig) transform for the current settings: points (mm → m) and directions. */
export const leapToHead = (calibration: NorthStarCalibration) => {
  const s = useCalibrationProfilesStore.getState().settings
  const rotation = HEAD_TO_RIG.clone()
    .multiply(degToQuat(s.handPitchOffset, s.handYawOffset, s.handRollOffset))
    .multiply(calibration.leapRotation)
    .multiply(LEAP_HMD_MOUNT)
  const offset = calibration.leapTranslation.clone()
    .add(new THREE.Vector3(s.handOffsetX, s.handOffsetY, s.handOffsetZ))
    .applyQuaternion(HEAD_TO_RIG)
  return {
    point: (posMm: Vec3) =>
      new THREE.Vector3().fromArray(posMm).multiplyScalar(0.001).applyQuaternion(rotation).add(offset),
    direction: (dir: Vec3) => new THREE.Vector3().fromArray(dir).applyQuaternion(rotation),
  }
}

/** Inverse of `leapToHead`: head-frame points (m) and directions → Leap sensor frame (mm). */
export const headToLeap = (calibration: NorthStarCalibration) => {
  const s = useCalibrationProfilesStore.getState().settings
  const rotation = HEAD_TO_RIG.clone()
    .multiply(degToQuat(s.handPitchOffset, s.handYawOffset, s.handRollOffset))
    .multiply(calibration.leapRotation)
    .multiply(LEAP_HMD_MOUNT)
    .invert()
  const offset = calibration.leapTranslation.clone()
    .add(new THREE.Vector3(s.handOffsetX, s.handOffsetY, s.handOffsetZ))
    .applyQuaternion(HEAD_TO_RIG)
  return {
    point: (p: THREE.Vector3) => p.clone().sub(offset).applyQuaternion(rotation).multiplyScalar(1000),
    direction: (d: THREE.Vector3) => d.clone().applyQuaternion(rotation),
  }
}
