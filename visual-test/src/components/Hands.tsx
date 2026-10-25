import { useMemo } from 'react'
import * as THREE from 'three'
import { useTrackingStore, type HandData } from '../stores/tracking'
import { leapToHead } from '../lib/transforms'
import type { NorthStarCalibration } from '../lib/calibration'

const JOINT_RADIUS = 0.006
const PALM_RADIUS = 0.012
// Occluders are sized like the real hand so holograms behind it get cut out.
const OCCLUDER_JOINT_RADIUS = 0.011
const OCCLUDER_PALM_RADIUS = 0.04

interface HandProps {
  data: HandData | null
  calibration: NorthStarCalibration
  /** Depth-only: invisible, but hides whatever is behind the real hand. */
  occluder: boolean
}

/** Renders a single hand (palm + finger joints) */
function Hand({ data, calibration, occluder }: HandProps) {
  const jointGeo = useMemo(
    () => new THREE.SphereGeometry(occluder ? OCCLUDER_JOINT_RADIUS : JOINT_RADIUS, 8, 8),
    [occluder],
  )
  const palmGeo = useMemo(
    () => new THREE.SphereGeometry(occluder ? OCCLUDER_PALM_RADIUS : PALM_RADIUS, 12, 12),
    [occluder],
  )

  if (!data) return null

  const toHead = leapToHead(calibration)
  const palmPos = toHead.point(data.palmPosition)
  const joints = data.fingers.flat().map((p) => toHead.point(p))
  const material = (color: number) => occluder
    ? <meshBasicMaterial colorWrite={false} />
    : <meshPhongMaterial color={color} emissive={color} emissiveIntensity={0.5} />

  return (
    <group renderOrder={occluder ? -1 : 0}>
      <mesh geometry={palmGeo} position={palmPos}>{material(0xff6600)}</mesh>
      {joints.map((pos, i) => (
        <mesh key={i} geometry={jointGeo} position={pos}>{material(0x00ff88)}</mesh>
      ))}
    </group>
  )
}

/** Renders both hands; must be a child of the head rig (head-relative). */
export function Hands({ calibration, occluder = false }: { calibration: NorthStarCalibration; occluder?: boolean }) {
  const leftHand = useTrackingStore((s) => s.leftHand)
  const rightHand = useTrackingStore((s) => s.rightHand)

  return (
    <>
      <Hand data={leftHand} calibration={calibration} occluder={occluder} />
      <Hand data={rightHand} calibration={calibration} occluder={occluder} />
    </>
  )
}
