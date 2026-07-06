import { useRef } from 'react'
import { useFrame } from '@react-three/fiber'
import type { Mesh } from 'three'

/** Shared demo scene: grid, lights, floating cubes for depth reference */
export function Scene() {
  return (
    <>
      <ambientLight intensity={0.4} />
      <directionalLight position={[5, 10, 5]} intensity={0.8} castShadow />
      <gridHelper args={[20, 40, 0x444466, 0x333355]} />
      <CalibrationAnchors />
      {Array.from({ length: 20 }, (_, i) => (
        <FloatingCube key={i} seed={i} />
      ))}
    </>
  )
}

function CalibrationAnchors() {
  const anchors = [
    { position: [0, 1.6, -0.5], size: 0.035, color: 0xff4444 },
    { position: [-0.18, 1.6, -1.0], size: 0.05, color: 0x00ff88 },
    { position: [0.18, 1.6, -1.0], size: 0.05, color: 0x66ccff },
    { position: [0, 1.6, -1.8], size: 0.075, color: 0xffffff },
  ] as const

  return (
    <>
      {anchors.map((anchor) => (
        <mesh key={anchor.color} position={anchor.position}>
          <sphereGeometry args={[anchor.size, 24, 24]} />
          <meshPhongMaterial color={anchor.color} emissive={anchor.color} emissiveIntensity={0.65} />
        </mesh>
      ))}
      <mesh position={[0, 1.6, -1.0]} rotation={[0, 0, Math.PI / 4]}>
        <boxGeometry args={[0.36, 0.006, 0.006]} />
        <meshBasicMaterial color={0xffffff} />
      </mesh>
      <mesh position={[0, 1.6, -1.0]} rotation={[0, 0, -Math.PI / 4]}>
        <boxGeometry args={[0.36, 0.006, 0.006]} />
        <meshBasicMaterial color={0xffffff} />
      </mesh>
    </>
  )
}

function FloatingCube({ seed }: { seed: number }) {
  const ref = useRef<Mesh>(null)
  const speed = 0.005 + (seed * 7.31 % 1) * 0.02

  useFrame(() => {
    if (ref.current) {
      ref.current.rotation.y += speed
      ref.current.rotation.x += speed * 0.5
    }
  })

  // Deterministic random placement from seed
  const x = ((seed * 13.37) % 4) - 2
  const y = ((seed * 7.91) % 2) + 0.5
  const z = ((seed * 3.53) % 4) - 2

  return (
    <mesh ref={ref} position={[x, y, z]}>
      <boxGeometry args={[0.1, 0.1, 0.1]} />
      <meshPhongMaterial color={0x00ffff} emissive={0x004444} />
    </mesh>
  )
}
