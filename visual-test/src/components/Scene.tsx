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
      {Array.from({ length: 20 }, (_, i) => (
        <FloatingCube key={i} seed={i} />
      ))}
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
