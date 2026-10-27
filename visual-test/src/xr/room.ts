/**
 * Real-room occluders from WebXR mesh detection: every `XRMesh` the runtime reports (the XR50
 * bridge's room mesh) becomes a depth-only mesh, so holograms behind real surfaces are cut out
 * like behind the real hand. `visible: true` draws a faint wireframe instead (`?mesh`).
 */
import * as THREE from 'three'

export interface RoomOccluder {
  update: (frame: XRFrame | undefined, space: XRReferenceSpace | null) => void
}

export const createRoomOccluder = (scene: THREE.Scene, { visible = false } = {}): RoomOccluder => {
  const material = visible
    ? new THREE.MeshBasicMaterial({ color: 0x3a6a7a, wireframe: true, transparent: true, opacity: 0.35 })
    : new THREE.MeshBasicMaterial({ colorWrite: false, side: THREE.DoubleSide })
  const group = new THREE.Group()
  scene.add(group)
  const meshes = new Map<XRMesh, THREE.Mesh>()

  const create = (xrMesh: XRMesh) => {
    const geometry = new THREE.BufferGeometry()
    geometry.setAttribute('position', new THREE.BufferAttribute(xrMesh.vertices, 3))
    geometry.setIndex(new THREE.BufferAttribute(xrMesh.indices, 1))
    const mesh = new THREE.Mesh(geometry, material)
    mesh.renderOrder = -1 // depth first, so everything drawn afterwards is tested against it
    mesh.matrixAutoUpdate = false
    group.add(mesh)
    meshes.set(xrMesh, mesh)
    return mesh
  }

  return {
    update: (frame, space) => {
      const detected = frame?.detectedMeshes
      if (!detected || !space) return
      for (const [xrMesh, mesh] of meshes) {
        if (detected.has(xrMesh)) continue
        group.remove(mesh)
        mesh.geometry.dispose()
        meshes.delete(xrMesh)
      }
      detected.forEach((xrMesh) => {
        const mesh = meshes.get(xrMesh) ?? create(xrMesh)
        const pose = frame.getPose(xrMesh.meshSpace, space)
        if (pose) mesh.matrix.fromArray(pose.transform.matrix)
      })
    },
  }
}
