import * as THREE from 'three'

// ---- Raw JSON shape from EskySettings.json ----
// Unity/Esky frame: X right, Y up, Z forward (left-handed).

interface Vec3 { x: number; y: number; z: number }
interface Vec4 { x: number; y: number; z: number; w: number }

interface Mat4Json {
  e00: number; e01: number; e02: number; e03: number
  e10: number; e11: number; e12: number; e13: number
  e20: number; e21: number; e22: number; e23: number
  e30: number; e31: number; e32: number; e33: number
}

interface EyeCalibJson {
  ellipseMinorAxis: number
  ellipseMajorAxis: number
  screenForward: Vec3
  screenPosition: Vec3
  eyePosition: Vec3
  eyeRotation: Vec4
  cameraProjection: Vec4 // tangent half-angles: x=left, y=right, z=top, w=bottom (see parseEye)
  sphereToWorldSpace: Mat4Json
  worldToScreenSpace: Mat4Json
}

interface EskySettingsJson {
  myOffsets: {
    TranslationEyeToLeapMotion: Vec3
    RotationEyeToLeapMotion: Vec4
    TranslationFromTracker: Vec3
    RotationFromTracker: Vec4
  }
  v2CalibrationValues: {
    left_uv_to_rect_x: number[]
    left_uv_to_rect_y: number[]
    right_uv_to_rect_x: number[]
    right_uv_to_rect_y: number[]
    left_eye_offset: number[]
    right_eye_offset: number[]
  }
  v1CalibrationValues: {
    leftEye: EyeCalibJson
    rightEye: EyeCalibJson
  }
  displayWindowSettings: {
    DisplayWidth: number
    DisplayHeight: number
    EyeTextureWidth: number
    EyeTextureHeight: number
  }
}

// ---- Parsed calibration types ----
// Everything below is in the Three.js head frame: X right, Y up, Z back (camera looks down -Z).

export interface EyeCalibration {
  eyePosition: THREE.Vector3
  eyeRotation: THREE.Quaternion
  screenPosition: THREE.Vector3
  screenForward: THREE.Vector3
  /** Asymmetric frustum as tangent half-angles */
  frustum: { left: number; right: number; bottom: number; top: number }
  /** Raw Unity-frame matrices, kept for the optics bench */
  worldToScreenSpace: THREE.Matrix4
  sphereToWorldSpace: THREE.Matrix4
  /** 16 polynomial coefficients for UV→rect X */
  uvToRectX: Float32Array
  /** 16 polynomial coefficients for UV→rect Y */
  uvToRectY: Float32Array
  eyeOffset: [number, number]
}

export interface NorthStarCalibration {
  leftEye: EyeCalibration
  rightEye: EyeCalibration
  /** Leap sensor pose in the head frame (sensor origin relative to eye center) */
  leapTranslation: THREE.Vector3
  leapRotation: THREE.Quaternion
  /** Head pose in the tracker (XR50) frame */
  trackerTranslation: THREE.Vector3
  trackerRotation: THREE.Quaternion
  /** Display dimensions */
  display: { width: number; height: number; eyeWidth: number; eyeHeight: number }
}

// ---- Parsing helpers ----

/** Unity is left-handed with Z forward: mirror Z for points, negate x/y for rotations. */
const unityVec3 = (v: Vec3) => new THREE.Vector3(v.x, v.y, -v.z)
const unityQuat = (q: Vec4) => new THREE.Quaternion(-q.x, -q.y, q.z, q.w)

const toMatrix4 = (m: Mat4Json): THREE.Matrix4 =>
  new THREE.Matrix4().set(
    m.e00, m.e01, m.e02, m.e03,
    m.e10, m.e11, m.e12, m.e13,
    m.e20, m.e21, m.e22, m.e23,
    m.e30, m.e31, m.e32, m.e33,
  )

const parseEye = (
  eye: EyeCalibJson,
  uvX: number[],
  uvY: number[],
  eyeOffset: number[],
): EyeCalibration => ({
  eyePosition: unityVec3(eye.eyePosition),
  eyeRotation: unityQuat(eye.eyeRotation),
  screenPosition: unityVec3(eye.screenPosition),
  screenForward: unityVec3(eye.screenForward),
  // Unity's ComposeProjection() (OpticalCalibrationManager.cs) builds the standard
  // off-center projection with top = z (+0.3: little FOV above the eye line) and
  // bottom = w (-1.4: most of the FOV below). Swapping them flips the eye image.
  frustum: {
    left: eye.cameraProjection.x,
    right: eye.cameraProjection.y,
    top: eye.cameraProjection.z,
    bottom: eye.cameraProjection.w,
  },
  worldToScreenSpace: toMatrix4(eye.worldToScreenSpace),
  sphereToWorldSpace: toMatrix4(eye.sphereToWorldSpace),
  uvToRectX: new Float32Array(uvX),
  uvToRectY: new Float32Array(uvY),
  eyeOffset: [eyeOffset[0] ?? 0, eyeOffset[1] ?? 0] as [number, number],
})

export const loadCalibration = async (url = '/calibration/EskySettings.json') =>
  parseCalibration(await (await fetch(url)).json())

export const parseCalibration = (json: EskySettingsJson): NorthStarCalibration => {
  const { v1CalibrationValues: v1, v2CalibrationValues: v2, myOffsets, displayWindowSettings: dw } = json

  return {
    leftEye: parseEye(v1.leftEye, v2.left_uv_to_rect_x, v2.left_uv_to_rect_y, v2.left_eye_offset),
    rightEye: parseEye(v1.rightEye, v2.right_uv_to_rect_x, v2.right_uv_to_rect_y, v2.right_eye_offset),
    leapTranslation: unityVec3(myOffsets.TranslationEyeToLeapMotion),
    leapRotation: unityQuat(myOffsets.RotationEyeToLeapMotion),
    trackerTranslation: unityVec3(myOffsets.TranslationFromTracker),
    trackerRotation: unityQuat(myOffsets.RotationFromTracker),
    display: {
      width: dw.DisplayWidth,
      height: dw.DisplayHeight,
      eyeWidth: dw.EyeTextureWidth,
      eyeHeight: dw.EyeTextureHeight,
    },
  }
}
