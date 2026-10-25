import { OrbitControls } from '@react-three/drei'
import { useEffect, useMemo } from 'react'
import { useThree } from '@react-three/fiber'
import * as THREE from 'three'
import { useCalibration } from '../hooks/useCalibration'

const makeCombinerGeometry = (side: -1 | 1) => {
  const cols = 28
  const rows = 24
  const vertices: number[] = []
  const indices: number[] = []

  for (let y = 0; y <= rows; y += 1) {
    for (let x = 0; x <= cols; x += 1) {
      const u = x / cols - 0.5
      const v = y / rows - 0.5
      const width = 0.13
      const height = 0.15
      const bow = 0.055 * (u * u * 1.2 + v * v)

      vertices.push(
        side * 0.055 + u * width,
        1.6 + v * height,
        -0.21 + bow,
      )
    }
  }

  for (let y = 0; y < rows; y += 1) {
    for (let x = 0; x < cols; x += 1) {
      const a = y * (cols + 1) + x
      const b = a + 1
      const c = a + cols + 1
      const d = c + 1
      indices.push(a, c, b, b, c, d)
    }
  }

  const geometry = new THREE.BufferGeometry()
  geometry.setAttribute('position', new THREE.Float32BufferAttribute(vertices, 3))
  geometry.setIndex(indices)
  geometry.computeVertexNormals()
  return geometry
}

const makeLineGeometry = (points: THREE.Vector3[]) => {
  const geometry = new THREE.BufferGeometry()
  geometry.setFromPoints(points)
  return geometry
}

function RayBundle({ side }: { side: -1 | 1 }) {
  const geometry = useMemo(() => {
    const eye = new THREE.Vector3(side * 0.03, 1.6, 0.02)
    const display = new THREE.Vector3(side * 0.1, 1.67, -0.04)
    const rays: THREE.Vector3[] = []

    for (const y of [-0.045, 0, 0.045]) {
      for (const x of [-0.035, 0, 0.035]) {
        const combiner = new THREE.Vector3(side * (0.055 + x), 1.6 + y, -0.205)
        rays.push(eye, combiner, combiner, display)
      }
    }

    return makeLineGeometry(rays)
  }, [side])

  return (
    <lineSegments geometry={geometry}>
      <lineBasicMaterial color={side < 0 ? 0x66ccff : 0xff9966} transparent opacity={0.8} />
    </lineSegments>
  )
}

function EyeMarker({ side }: { side: -1 | 1 }) {
  return (
    <mesh position={[side * 0.03, 1.6, 0.02]}>
      <sphereGeometry args={[0.009, 16, 16]} />
      <meshStandardMaterial color={0xffffff} emissive={0x333333} />
    </mesh>
  )
}

function DisplayPanel({ side }: { side: -1 | 1 }) {
  return (
    <mesh position={[side * 0.1, 1.67, -0.04]} rotation={[0.35, side * 0.3, 0]}>
      <planeGeometry args={[0.12, 0.085]} />
      <meshBasicMaterial color={side < 0 ? 0x224488 : 0x884422} transparent opacity={0.75} />
    </mesh>
  )
}

function Combiner({ side }: { side: -1 | 1 }) {
  const geometry = useMemo(() => makeCombinerGeometry(side), [side])

  return (
    <mesh geometry={geometry}>
      <meshPhysicalMaterial
        color={0x99ddff}
        transparent
        opacity={0.24}
        roughness={0.08}
        metalness={0.15}
        side={THREE.DoubleSide}
      />
    </mesh>
  )
}

export function OpticsBench() {
  const calibration = useCalibration()
  const ipd = calibration
    ? Math.abs(calibration.rightEye.eyePosition.x - calibration.leftEye.eyePosition.x)
    : 0.06
  const { camera } = useThree()

  useEffect(() => {
    camera.position.set(0, 1.72, 0.42)
    camera.lookAt(0, 1.6, -0.14)
    camera.updateProjectionMatrix()
  }, [camera])

  return (
    <>
      <color attach="background" args={['#050505']} />
      <ambientLight intensity={0.7} />
      <directionalLight position={[0, 2.2, 0.8]} intensity={1.2} />
      <gridHelper args={[1, 20, 0x333333, 0x181818]} position={[0, 1.42, -0.16]} />

      <group>
        <EyeMarker side={-1} />
        <EyeMarker side={1} />
        <DisplayPanel side={-1} />
        <DisplayPanel side={1} />
        <Combiner side={-1} />
        <Combiner side={1} />
        <RayBundle side={-1} />
        <RayBundle side={1} />
      </group>

      <mesh position={[0, 1.6, 0.02]} scale={[ipd, 0.002, 0.002]}>
        <boxGeometry args={[1, 1, 1]} />
        <meshBasicMaterial color={0x00ff88} />
      </mesh>

      <OrbitControls target={[0, 1.6, -0.13]} enableDamping />
    </>
  )
}
