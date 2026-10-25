import { useRef, type ReactNode } from 'react'
import { useFrame } from '@react-three/fiber'
import * as THREE from 'three'
import { useTrackingStore } from '../stores/tracking'
import { headPoseToRig } from '../lib/transforms'
import type { NorthStarCalibration } from '../lib/calibration'

/** A group that follows the tracked head pose; head-relative content goes inside. */
export function HeadRig({ calibration, children }: { calibration: NorthStarCalibration; children: ReactNode }) {
  const ref = useRef<THREE.Group>(null)

  useFrame(() => {
    if (!ref.current) return
    const rig = headPoseToRig(useTrackingStore.getState().headPose, calibration)
    ref.current.position.copy(rig.position)
    ref.current.quaternion.copy(rig.quaternion)
  })

  return <group ref={ref}>{children}</group>
}
