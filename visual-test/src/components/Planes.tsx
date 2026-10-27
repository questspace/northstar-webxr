import { useMemo } from 'react'
import * as THREE from 'three'
import { useTrackingStore } from '../stores/tracking'
import type { WorldPlane } from '../lib/transforms'

/** Fan triangulation of a border polygon around its centroid (the SDK's borders are near-convex). */
const fillGeometry = (border: THREE.Vector3[]) => {
  const c = border.reduce((s, p) => s.add(p), new THREE.Vector3()).divideScalar(border.length)
  const positions = border.flatMap((p, i) => {
    const q = border[(i + 1) % border.length]
    return [c.x, c.y, c.z, p.x, p.y, p.z, q.x, q.y, q.z]
  })
  const geometry = new THREE.BufferGeometry()
  geometry.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3))
  return geometry
}

const outlineGeometry = (border: THREE.Vector3[]) => new THREE.BufferGeometry().setFromPoints([...border, border[0]])

const occluderMaterial = new THREE.MeshBasicMaterial({ colorWrite: false, side: THREE.DoubleSide })
const outlineMaterial = new THREE.LineBasicMaterial({ color: 0x3a6a7a, transparent: true, opacity: 0.6 })

function Plane({ plane }: { plane: WorldPlane }) {
  const fill = useMemo(() => fillGeometry(plane.border), [plane.border])
  const outline = useMemo(() => outlineGeometry(plane.border), [plane.border])
  return (
    <group>
      {/* Depth first: holograms behind a real surface are cut out, like behind the real hand. */}
      <mesh geometry={fill} material={occluderMaterial} renderOrder={-1} />
      <lineLoop geometry={outline} material={outlineMaterial} />
    </group>
  )
}

/** Surfaces the XR50 detected (bridge SDK source), in world space. Renders nothing without them. */
export function Planes() {
  const planes = useTrackingStore((s) => s.planes)
  return <>{planes.filter((p) => p.border.length >= 3).map((p) => <Plane key={p.id} plane={p} />)}</>
}
