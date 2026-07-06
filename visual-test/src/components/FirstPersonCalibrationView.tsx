import { useRef } from 'react'
import { useFrame, useThree } from '@react-three/fiber'
import * as THREE from 'three'
import { useTrackingStore } from '../stores/tracking'
import { xr50ToEuler, xr50ToScene } from '../lib/transforms'
import { Scene } from './Scene'
import { Hands } from './Hands'

const cameraCorrection = new THREE.Quaternion().setFromEuler(
  new THREE.Euler(Math.PI, 0, 0, 'YXZ'),
)
const correctedQuat = new THREE.Quaternion()

/**
 * First-person calibration view from XR50 head pose.
 * This is the camera-centric view used to tune tracking/optics alignment.
 */
export function FirstPersonCalibrationView() {
  const rigRef = useRef<THREE.Group>(null)
  const { camera } = useThree()

  useFrame(() => {
    if (!rigRef.current) return

    const { headPose } = useTrackingStore.getState()
    rigRef.current.position.copy(xr50ToScene(headPose.x, headPose.y, headPose.z))
    rigRef.current.rotation.copy(xr50ToEuler(headPose.roll, headPose.pitch, headPose.yaw))
    rigRef.current.updateMatrixWorld(true)

    camera.position.copy(rigRef.current.position)
    correctedQuat.copy(rigRef.current.quaternion).multiply(cameraCorrection)
    camera.quaternion.copy(correctedQuat)
    camera.updateMatrixWorld(true)
  })

  return (
    <>
      <Scene />
      <group ref={rigRef}>
        <Hands />
      </group>
    </>
  )
}
