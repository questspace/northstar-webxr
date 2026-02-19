import { useMemo } from 'react'
import * as THREE from 'three'
import { useTrackingStore, type HandData } from '../stores/tracking'
import { leapToScene } from '../lib/transforms'

const JOINT_RADIUS = 0.006
const PALM_RADIUS = 0.012

/** Renders a single hand (palm + finger joints) */
function Hand({ data }: { data: HandData | null }) {
  const jointGeo = useMemo(() => new THREE.SphereGeometry(JOINT_RADIUS, 8, 8), [])
  const palmGeo = useMemo(() => new THREE.SphereGeometry(PALM_RADIUS, 12, 12), [])

  if (!data) return null

  const palmPos = leapToScene(data.palmPosition)
  const joints = data.fingers.flatMap((finger) =>
    finger.map((j) => leapToScene(j.position)),
  )

  return (
    <group>
      <mesh geometry={palmGeo} position={palmPos}>
        <meshPhongMaterial color={0xff6600} emissive={0xff6600} emissiveIntensity={0.5} />
      </mesh>
      {joints.map((pos, i) => (
        <mesh key={i} geometry={jointGeo} position={pos}>
          <meshPhongMaterial color={0x00ff88} emissive={0x00ff88} emissiveIntensity={0.5} />
        </mesh>
      ))}
    </group>
  )
}

/** Renders both hands as children of the camera rig (head-relative) */
export function Hands() {
  const leftHand = useTrackingStore((s) => s.leftHand)
  const rightHand = useTrackingStore((s) => s.rightHand)

  return (
    <>
      <Hand data={leftHand} />
      <Hand data={rightHand} />
    </>
  )
}
