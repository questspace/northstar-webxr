import * as THREE from 'three'

// ---- Raw JSON shape from EskySettings.json ----

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
  cameraProjection: Vec4 // x=left, y=right, z=bottom, w=top
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

export interface EyeCalibration {
  eyePosition: THREE.Vector3
  eyeRotation: THREE.Quaternion
  screenPosition: THREE.Vector3
  screenForward: THREE.Vector3
  /** Asymmetric frustum: left, right, bottom, top (in tangent-space) */
  frustum: { left: number; right: number; bottom: number; top: number }
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
  /** Translation from eye center to Leap sensor (meters) */
  leapTranslation: THREE.Vector3
  /** Rotation from eye space to Leap sensor space */
  leapRotation: THREE.Quaternion
  /** Translation from tracker to display */
  trackerTranslation: THREE.Vector3
  /** Rotation from tracker to display */
  trackerRotation: THREE.Quaternion
  /** Display dimensions */
  display: { width: number; height: number; eyeWidth: number; eyeHeight: number }
}

// ---- Parsing helpers ----

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
  eyePosition: new THREE.Vector3(eye.eyePosition.x, eye.eyePosition.y, eye.eyePosition.z),
  eyeRotation: new THREE.Quaternion(eye.eyeRotation.x, eye.eyeRotation.y, eye.eyeRotation.z, eye.eyeRotation.w),
  screenPosition: new THREE.Vector3(eye.screenPosition.x, eye.screenPosition.y, eye.screenPosition.z),
  screenForward: new THREE.Vector3(eye.screenForward.x, eye.screenForward.y, eye.screenForward.z),
  frustum: {
    left: eye.cameraProjection.x,
    right: eye.cameraProjection.y,
    bottom: eye.cameraProjection.z,
    top: eye.cameraProjection.w,
  },
  worldToScreenSpace: toMatrix4(eye.worldToScreenSpace),
  sphereToWorldSpace: toMatrix4(eye.sphereToWorldSpace),
  uvToRectX: new Float32Array(uvX),
  uvToRectY: new Float32Array(uvY),
  eyeOffset: [eyeOffset[0] ?? 0, eyeOffset[1] ?? 0] as [number, number],
})

export const parseCalibration = (json: EskySettingsJson): NorthStarCalibration => {
  const { v1CalibrationValues: v1, v2CalibrationValues: v2, myOffsets, displayWindowSettings: dw } = json

  return {
    leftEye: parseEye(v1.leftEye, v2.left_uv_to_rect_x, v2.left_uv_to_rect_y, v2.left_eye_offset),
    rightEye: parseEye(v1.rightEye, v2.right_uv_to_rect_x, v2.right_uv_to_rect_y, v2.right_eye_offset),
    leapTranslation: new THREE.Vector3(
      myOffsets.TranslationEyeToLeapMotion.x,
      myOffsets.TranslationEyeToLeapMotion.y,
      myOffsets.TranslationEyeToLeapMotion.z,
    ),
    leapRotation: new THREE.Quaternion(
      myOffsets.RotationEyeToLeapMotion.x,
      myOffsets.RotationEyeToLeapMotion.y,
      myOffsets.RotationEyeToLeapMotion.z,
      myOffsets.RotationEyeToLeapMotion.w,
    ),
    trackerTranslation: new THREE.Vector3(
      myOffsets.TranslationFromTracker.x,
      myOffsets.TranslationFromTracker.y,
      myOffsets.TranslationFromTracker.z,
    ),
    trackerRotation: new THREE.Quaternion(
      myOffsets.RotationFromTracker.x,
      myOffsets.RotationFromTracker.y,
      myOffsets.RotationFromTracker.z,
      myOffsets.RotationFromTracker.w,
    ),
    display: {
      width: dw.DisplayWidth,
      height: dw.DisplayHeight,
      eyeWidth: dw.EyeTextureWidth,
      eyeHeight: dw.EyeTextureHeight,
    },
  }
}
