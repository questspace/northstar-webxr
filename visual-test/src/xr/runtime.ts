import * as THREE from 'three'
import { mat4 } from 'gl-matrix'
import { XRDevice, type XRDeviceConfig } from 'iwer/lib/device/XRDevice.js'
import { XREnvironmentBlendMode, XRInteractionMode, type XRSession } from 'iwer/lib/session/XRSession.js'
import type { XRSystem } from 'iwer/lib/initialization/XRSystem.js'
import type { XRHandInput } from 'iwer/lib/device/XRHandInput.js'
import { XRHandJoint } from 'iwer/lib/input/XRHand.js'
import { XRHandedness } from 'iwer/lib/input/XRInputSource.js'
import { XREye } from 'iwer/lib/views/XRView.js'
import { P_DEVICE, P_JOINT_SPACE, P_SESSION, P_SPACE } from 'iwer/lib/private.js'
import type { NorthStarCalibration } from '../lib/calibration'
import { headPoseToRig, leapToHead } from '../lib/transforms'
import { useTrackingStore, type HandData, type Vec3 } from '../stores/tracking'
import { useCalibrationProfilesStore } from '../stores/calibrationProfiles'
import { NorthStarEnvironment } from './environment'
import { createWarpPass, eyeProjection, reprojection, warpParams, type WarpPass } from './optics'

/**
 * A WebXR runtime for Project NorthStar built on IWER (Meta's Immersive Web
 * Emulation Runtime). IWER provides the whole WebXR Device API; this adapter
 * drives it from the XR50 head pose and Leap hands, replaces the symmetric
 * fovy/ipd views with the calibrated eye cameras, forces the panel resolution,
 * and warps each finished frame through the combiner optics.
 */
export interface NorthStarXR {
  device: XRDevice
  warpEnabled: boolean
  reprojectionEnabled: boolean
  uninstall: () => void
}

const config: XRDeviceConfig = {
  name: 'Project NorthStar',
  controllerConfig: undefined,
  supportedSessionModes: ['inline', 'immersive-vr', 'immersive-ar'],
  supportedFeatures: ['viewer', 'local', 'local-floor', 'unbounded', 'anchors', 'hit-test', 'hand-tracking', 'plane-detection', 'mesh-detection'],
  supportedFrameRates: [60, 120],
  isSystemKeyboardSupported: false,
  internalNominalFrameRate: 120,
  environmentBlendModes: {
    inline: XREnvironmentBlendMode.Opaque,
    'immersive-vr': XREnvironmentBlendMode.Opaque,
    // Optical see-through: black adds no light, so the real world shows through.
    'immersive-ar': XREnvironmentBlendMode.Additive,
  },
  interactionMode: XRInteractionMode.WorldSpace,
  userAgent: `${navigator.userAgent} NorthStar/IWER`,
}

/** WebXR joint names per finger (thumb has no intermediate phalanx). */
const FINGER_JOINTS: XRHandJoint[][] = [
  [XRHandJoint.ThumbMetacarpal, XRHandJoint.ThumbPhalanxProximal, XRHandJoint.ThumbPhalanxDistal, XRHandJoint.ThumbTip],
  [XRHandJoint.IndexFingerMetacarpal, XRHandJoint.IndexFingerPhalanxProximal, XRHandJoint.IndexFingerPhalanxIntermediate, XRHandJoint.IndexFingerPhalanxDistal, XRHandJoint.IndexFingerTip],
  [XRHandJoint.MiddleFingerMetacarpal, XRHandJoint.MiddleFingerPhalanxProximal, XRHandJoint.MiddleFingerPhalanxIntermediate, XRHandJoint.MiddleFingerPhalanxDistal, XRHandJoint.MiddleFingerTip],
  [XRHandJoint.RingFingerMetacarpal, XRHandJoint.RingFingerPhalanxProximal, XRHandJoint.RingFingerPhalanxIntermediate, XRHandJoint.RingFingerPhalanxDistal, XRHandJoint.RingFingerTip],
  [XRHandJoint.PinkyFingerMetacarpal, XRHandJoint.PinkyFingerPhalanxProximal, XRHandJoint.PinkyFingerPhalanxIntermediate, XRHandJoint.PinkyFingerPhalanxDistal, XRHandJoint.PinkyFingerTip],
]
/** Leap joints [carp, mcp, pip, dip, tip] feeding the WebXR joints above. */
const LEAP_JOINTS = [[1, 2, 3, 4], [0, 1, 2, 3, 4], [0, 1, 2, 3, 4], [0, 1, 2, 3, 4], [0, 1, 2, 3, 4]]
const WRIST_RADIUS = 0.02

/** WebXR joint frame: -Z along the bone (toward the next joint), +Y out of the back of the hand. */
const jointMatrix = (position: THREE.Vector3, bone: THREE.Vector3, back: THREE.Vector3) => {
  const z = bone.clone().normalize().negate()
  const x = new THREE.Vector3().crossVectors(back, z).normalize()
  const y = new THREE.Vector3().crossVectors(z, x)
  return new THREE.Matrix4().makeBasis(x, y, z).setPosition(position)
}

const updateHand = (input: XRHandInput, data: HandData | null, calibration: NorthStarCalibration, rig: THREE.Matrix4, rigQuat: THREE.Quaternion) => {
  input.connected = data !== null
  if (!data) return

  const leap = leapToHead(calibration)
  const world = (mm: Vec3) => leap.point(mm).applyMatrix4(rig)
  const worldDir = (dir: Vec3) => leap.direction(dir).applyQuaternion(rigQuat)
  const back = worldDir(data.palmNormal).negate()

  // The target ray is the wrist pose pointing along the fingers.
  const wrist = world(data.wrist)
  const wristMatrix = jointMatrix(wrist, worldDir(data.direction), back)
  const wristQuat = new THREE.Quaternion().setFromRotationMatrix(wristMatrix)
  input.position.set(wrist.x, wrist.y, wrist.z)
  input.quaternion.set(wristQuat.x, wristQuat.y, wristQuat.z, wristQuat.w)
  input.updateTargetRaySpace()

  const rayInverse = wristMatrix.clone().invert()
  const setJoint = (name: XRHandJoint, matrix: THREE.Matrix4, radius: number) => {
    const space = input.inputSource.hand!.get(name)!
    mat4.copy(space[P_SPACE].offsetMatrix, rayInverse.clone().multiply(matrix).elements)
    space[P_JOINT_SPACE].radius = radius
  }
  setJoint(XRHandJoint.Wrist, wristMatrix, WRIST_RADIUS)
  data.fingers.forEach((joints, finger) => {
    const points = LEAP_JOINTS[finger].map((i) => world(joints[i]))
    points.forEach((point, k) => {
      const bone = k + 1 < points.length
        ? points[k + 1].clone().sub(point)
        : point.clone().sub(points[k - 1])
      setJoint(FINGER_JOINTS[finger][k], jointMatrix(point, bone, back), data.fingerWidth[finger] * 0.0005)
    })
  })
  input.updatePinchValue(data.pinchStrength)
}

/** Per-session: calibrated projection and the optics pass after each frame. */
const attachOptics = (session: XRSession, calibration: NorthStarCalibration, xr: NorthStarXR, renderQuat: THREE.Quaternion) => {
  const s = session[P_SESSION]
  const projection = new THREE.Matrix4()
  let near = 0
  let far = 0
  s.getProjectionMatrix = (eye) => {
    if (eye === XREye.None) return s.projectionMatrices[eye]
    const { depthNear, depthFar } = s.renderState
    if (depthNear !== near || depthFar !== far) {
      eyeProjection(calibration, depthNear, depthFar, projection)
      near = depthNear
      far = depthFar
    }
    return projection.elements
  }

  let pass: WarpPass | null = null
  const stats = createFrameStats('[XR]')
  const deviceFrame = s.onDeviceFrame
  s.onDeviceFrame = () => {
    deviceFrame()
    const layer = s.renderState.baseLayer
    if (s.ended || !layer || s.mode === 'inline') return
    pass ??= createWarpPass(layer.context, calibration)
    const settings = useCalibrationProfilesStore.getState().settings
    const latest = useTrackingStore.getState().headPose
    const display = headPoseToRig(latest, calibration).quaternion
    pass.warpCanvas(warpParams(
      settings,
      xr.warpEnabled ? settings.distortionStrength : 0,
      xr.reprojectionEnabled ? reprojection(renderQuat, display) : undefined,
    ))
    stats.present(latest.receivedAt)
  }
}

/** Once a second: frame rate, worst frame interval and head-pose age at the warp pass. */
const createFrameStats = (tag: string) => {
  let frames = 0
  let ageSum = 0
  let ageMax = 0
  let intervalMax = 0
  let last = 0
  let tick = performance.now()
  return {
    present(receivedAt: number) {
      const now = performance.now()
      if (last) intervalMax = Math.max(intervalMax, now - last)
      last = now
      const age = receivedAt ? Date.now() - receivedAt : 0
      ageSum += age
      ageMax = Math.max(ageMax, age)
      frames++
      if (now - tick < 1000) return
      console.log(`${tag} ${(frames / ((now - tick) / 1000)).toFixed(0)} fps  frame max ${intervalMax.toFixed(1)} ms  pose age avg ${(ageSum / frames).toFixed(2)} ms max ${ageMax.toFixed(2)} ms`)
      frames = ageSum = ageMax = intervalMax = 0
      tick = now
    },
  }
}

let installed: NorthStarXR | null = null

/** Installs `navigator.xr` for this NorthStar. Idempotent. */
export const installNorthStarXR = (calibration: NorthStarCalibration): NorthStarXR => {
  if (installed) return installed

  const device = new XRDevice(config, { stereoEnabled: true })
  device.primaryInputMode = 'hand'
  device.controlMode = 'programmatic'
  const d = device[P_DEVICE]
  const renderQuat = new THREE.Quaternion()
  const rig = new THREE.Matrix4()

  // Viewer = tracked head pose; views = calibrated eye positions.
  d.updateViews = () => {
    const pose = headPoseToRig(useTrackingStore.getState().headPose, calibration)
    renderQuat.copy(pose.quaternion)
    rig.compose(pose.position, pose.quaternion, new THREE.Vector3(1, 1, 1))
    device.position.set(pose.position.x, pose.position.y, pose.position.z)
    device.quaternion.set(pose.quaternion.x, pose.quaternion.y, pose.quaternion.z, pose.quaternion.w)
    mat4.fromRotationTranslation(d.viewerSpace[P_SPACE].offsetMatrix, device.quaternion.quat, device.position.vec3)
    mat4.fromTranslation(d.viewSpaces[XREye.Left][P_SPACE].offsetMatrix, calibration.leftEye.eyePosition.toArray())
    mat4.fromTranslation(d.viewSpaces[XREye.Right][P_SPACE].offsetMatrix, calibration.rightEye.eyePosition.toArray())
  }

  // The app canvas is the panel: one eye per half, regardless of the window.
  const baseLayerSet = d.onBaseLayerSet
  d.onBaseLayerSet = (layer) => {
    baseLayerSet(layer)
    if (!layer) return
    const canvas = layer.context.canvas as HTMLCanvasElement
    canvas.width = calibration.display.width
    canvas.height = calibration.display.height
    // Fill the screen in CSS pixels so Windows display scaling cannot crop the panel image.
    canvas.style.width = '100vw'
    canvas.style.height = '100vh'
  }

  // Leap joints go in after IWER's own pose-based skeleton update.
  const frameStart = d.onFrameStart
  d.onFrameStart = (frame) => {
    frameStart(frame)
    const { leftHand, rightHand } = useTrackingStore.getState()
    updateHand(device.hands[XRHandedness.Left]!, leftHand, calibration, rig, renderQuat)
    updateHand(device.hands[XRHandedness.Right]!, rightHand, calibration, rig, renderQuat)
  }

  // Desktop Chromium exposes a native navigator.xr (OpenXR) that knows nothing
  // about this headset; the emulated runtime must replace it.
  // Planes, meshes and hit tests come from the real room (XR50 bridge), not a synthetic one.
  device.installSEM(NorthStarEnvironment)
  device.installRuntime({ forceInstall: true })
  const xr: NorthStarXR = {
    device,
    warpEnabled: true,
    reprojectionEnabled: true,
    uninstall: () => {
      device.uninstallRuntime()
      installed = null
    },
  }
  const system = navigator.xr as unknown as XRSystem
  const requestSession = system.requestSession.bind(system)
  system.requestSession = async (mode, options) => {
    const session = await requestSession(mode, options)
    attachOptics(session, calibration, xr, renderQuat)
    return session
  }

  installed = xr
  return xr
}
