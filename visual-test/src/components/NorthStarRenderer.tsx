import { useEffect, useMemo, useRef } from 'react'
import { useFrame, useThree } from '@react-three/fiber'
import * as THREE from 'three'
import { useTrackingStore } from '../stores/tracking'
import { useCalibrationProfilesStore } from '../stores/calibrationProfiles'
import { headPoseToRig } from '../lib/transforms'
import type { NorthStarCalibration } from '../lib/calibration'
import { createWarpPass, eyeProjection, reprojection, warpParams } from '../xr/optics'

const NEAR = 0.01
const FAR = 1000

/**
 * Panel preview without WebXR (simulator): renders the R3F scene from the two
 * calibrated eye cameras side by side, then warps the canvas through the
 * optics exactly like the XR runtime does for a WebXR app.
 * Keys: U toggles the warp, R the re-projection, M mirrors the canvas.
 */
export function NorthStarRenderer({ calibration }: { calibration: NorthStarCalibration }) {
  const gl = useThree((s) => s.gl)
  const scene = useThree((s) => s.scene)
  const warpRef = useRef(true)
  const reprojectionRef = useRef(true)
  const renderQuat = useMemo(() => new THREE.Quaternion(), [])

  const eyes = useMemo(() => [calibration.leftEye, calibration.rightEye].map((eye) => {
    const camera = new THREE.PerspectiveCamera()
    eyeProjection(calibration, NEAR, FAR, camera.projectionMatrix)
    camera.projectionMatrixInverse.copy(camera.projectionMatrix).invert()
    return { camera, offset: eye.eyePosition }
  }), [calibration])

  const pass = useMemo(() => createWarpPass(gl.getContext(), calibration), [gl, calibration])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const key = e.key.toLowerCase()
      if (key === 'u') warpRef.current = !warpRef.current
      if (key === 'r') reprojectionRef.current = !reprojectionRef.current
      if (key === 'm') {
        const mirrored = gl.domElement.style.transform === 'scaleX(-1)'
        gl.domElement.style.transform = mirrored ? 'none' : 'scaleX(-1)'
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [gl])

  useFrame(() => {
    const rig = headPoseToRig(useTrackingStore.getState().headPose, calibration)
    renderQuat.copy(rig.quaternion)
    const { width, height } = gl.getDrawingBufferSize(new THREE.Vector2())

    gl.setScissorTest(true)
    eyes.forEach(({ camera, offset }, i) => {
      camera.position.copy(offset).applyQuaternion(rig.quaternion).add(rig.position)
      camera.quaternion.copy(rig.quaternion)
      gl.setViewport(i * width / 2, 0, width / 2, height)
      gl.setScissor(i * width / 2, 0, width / 2, height)
      gl.render(scene, camera)
    })
    gl.setScissorTest(false)

    const settings = useCalibrationProfilesStore.getState().settings
    const display = headPoseToRig(useTrackingStore.getState().headPose, calibration).quaternion
    pass.warpCanvas(warpParams(
      settings,
      warpRef.current ? settings.distortionStrength : 0,
      reprojectionRef.current ? reprojection(renderQuat, display) : undefined,
    ))
    gl.resetState()
  }, 1)

  return null
}
