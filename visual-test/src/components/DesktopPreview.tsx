import { useRef } from 'react'
import { useFrame } from '@react-three/fiber'
import { OrbitControls } from '@react-three/drei'
import * as THREE from 'three'
import { useTrackingStore } from '../stores/tracking'
import { xr50ToScene, xr50ToEuler } from '../lib/transforms'
import { Scene } from './Scene'
import { Hands } from './Hands'

/** NorthStar headset avatar mesh */
function HeadsetAvatar() {
  const ref = useRef<THREE.Group>(null)

  useFrame(() => {
    if (!ref.current) return
    const { headPose } = useTrackingStore.getState()
    ref.current.position.copy(xr50ToScene(headPose.x, headPose.y, headPose.z))
    ref.current.rotation.copy(xr50ToEuler(headPose.roll, headPose.pitch, headPose.yaw))
  })

  return (
    <group ref={ref}>
      {/* Visor body */}
      <mesh>
        <boxGeometry args={[0.18, 0.06, 0.08]} />
        <meshStandardMaterial color={0x1a1a2e} roughness={0.3} metalness={0.8} />
      </mesh>
      {/* Left lens */}
      <mesh position={[-0.04, 0, 0.041]}>
        <circleGeometry args={[0.025, 32]} />
        <meshPhongMaterial color={0x00aaff} emissive={0x00aaff} emissiveIntensity={0.5} side={THREE.DoubleSide} />
      </mesh>
      {/* Right lens */}
      <mesh position={[0.04, 0, 0.041]}>
        <circleGeometry args={[0.025, 32]} />
        <meshPhongMaterial color={0x00aaff} emissive={0x00aaff} emissiveIntensity={0.5} side={THREE.DoubleSide} />
      </mesh>
      {/* Direction arrow */}
      <mesh position={[0, 0, 0.07]} rotation={[Math.PI / 2, 0, 0]}>
        <coneGeometry args={[0.01, 0.03, 8]} />
        <meshPhongMaterial color={0x00ff88} emissive={0x00ff88} emissiveIntensity={0.5} />
      </mesh>
      {/* Hands are children of headset (head-relative) */}
      <Hands />
    </group>
  )
}

/** Desktop debug preview with orbit controls and 3rd-person view of headset */
export function DesktopPreview() {
  return (
    <>
      <OrbitControls makeDefault enableDamping />
      <Scene />
      <HeadsetAvatar />
    </>
  )
}
