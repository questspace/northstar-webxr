import { useEffect, useMemo } from 'react'
import * as THREE from 'three'
import { useTrackingStore } from '../stores/tracking'
import type { MeshBlock } from '../lib/transforms'

const occluderMaterial = new THREE.MeshBasicMaterial({ colorWrite: false, side: THREE.DoubleSide })
const wireMaterial = new THREE.MeshBasicMaterial({ color: 0x3a6a7a, wireframe: true, transparent: true, opacity: 0.35 })
/** `?mesh` shows the mesh as a faint wireframe (it only occludes otherwise). */
const showMesh = new URLSearchParams(location.search).has('mesh')

function Block({ block }: { block: MeshBlock }) {
  const geometry = useMemo(() => {
    const g = new THREE.BufferGeometry()
    g.setAttribute('position', new THREE.BufferAttribute(block.positions, 3))
    g.setIndex(new THREE.BufferAttribute(block.indices, 1))
    return g
  }, [block])
  useEffect(() => () => geometry.dispose(), [geometry])
  return (
    <>
      {/* Depth first: holograms behind real surfaces are cut out, like behind the real hand. */}
      <mesh geometry={geometry} material={occluderMaterial} renderOrder={-1} />
      {showMesh && <mesh geometry={geometry} material={wireMaterial} />}
    </>
  )
}

/** The room mesh from the bridge's recon source, in world space. Renders nothing without it. */
export function RoomMesh() {
  const blocks = useTrackingStore((s) => s.meshBlocks)
  return <>{[...blocks.values()].map((b) => <Block key={b.key} block={b} />)}</>
}
