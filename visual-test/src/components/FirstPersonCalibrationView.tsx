import { useFrame, useThree } from '@react-three/fiber'
import { useTrackingStore } from '../stores/tracking'
import { headPoseToRig } from '../lib/transforms'
import type { NorthStarCalibration } from '../lib/calibration'
import { Scene } from './Scene'
import { HeadRig } from './HeadRig'
import { Hands } from './Hands'

/**
 * First-person calibration view from XR50 head pose.
 * This is the camera-centric view used to tune tracking/optics alignment.
 */
export function FirstPersonCalibrationView({ calibration }: { calibration: NorthStarCalibration }) {
  const camera = useThree((s) => s.camera)

  useFrame(() => {
    const rig = headPoseToRig(useTrackingStore.getState().headPose, calibration)
    camera.position.copy(rig.position)
    camera.quaternion.copy(rig.quaternion)
  })

  return (
    <>
      <Scene />
      <HeadRig calibration={calibration}>
        <Hands calibration={calibration} />
      </HeadRig>
    </>
  )
}
