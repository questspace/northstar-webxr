import * as THREE from 'three'
import type { mat4 } from 'gl-matrix'
import type { SyntheticEnvironmentModule, XRDevice } from 'iwer/lib/device/XRDevice.js'
import { NativePlane } from 'iwer/lib/planes/XRPlane.js'
import { NativeMesh } from 'iwer/lib/meshes/XRMesh.js'
import { XRRigidTransform } from 'iwer/lib/primitives/XRRigidTransform.js'
import { XRSemanticLabels } from 'iwer/lib/labels/labels.js'
import { useTrackingStore, type TrackingState } from '../stores/tracking'
import type { MeshBlock, WorldPlane } from '../lib/transforms'

/** A plane in its own frame: +Y along the normal, the polygon in its XZ plane. */
interface LocalPlane {
  native: NativePlane
  center: THREE.Vector3
  rotation: THREE.Quaternion
  inverse: THREE.Quaternion
  normal: THREE.Vector3
  polygon: THREE.Vector2[]
}

const UP = new THREE.Vector3(0, 1, 0)

const toLocalPlane = (plane: WorldPlane): LocalPlane => {
  const center = plane.border.reduce((s, p) => s.add(p), new THREE.Vector3()).divideScalar(plane.border.length)
  const normal = plane.normal.clone().normalize()
  const rotation = new THREE.Quaternion().setFromUnitVectors(UP, normal)
  const inverse = rotation.clone().invert()
  const local = plane.border.map((p) => p.clone().sub(center).applyQuaternion(inverse))
  const horizontal = Math.abs(normal.y) > 0.7
  const native = new NativePlane(
    new XRRigidTransform(center, { x: rotation.x, y: rotation.y, z: rotation.z, w: rotation.w }),
    local.map((p) => new DOMPointReadOnly(p.x, 0, p.z)),
    // Height alone cannot tell a table from the floor here; walls are walls.
    horizontal ? XRSemanticLabels.Other : XRSemanticLabels.Wall,
  )
  return { native, center, rotation, inverse, normal, polygon: local.map((p) => new THREE.Vector2(p.x, p.z)) }
}

const inside = (p: THREE.Vector2, polygon: THREE.Vector2[]) => {
  let hit = false
  for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i++) {
    const [a, b] = [polygon[i], polygon[j]]
    if (a.y > p.y !== b.y > p.y && p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x) hit = !hit
  }
  return hit
}

const IDENTITY = new XRRigidTransform()

/**
 * IWER's synthetic environment module, fed by the real room: the XR50 bridge's planes and
 * mesh blocks (tracking store, world space) become WebXR `plane-detection`,
 * `mesh-detection` and `hit-test` results for any app on this runtime.
 */
export class NorthStarEnvironment implements SyntheticEnvironmentModule {
  version = 'northstar-1'
  planesVisible = false
  boundingBoxesVisible = false
  meshesVisible = false
  readonly environmentCanvas = Object.assign(document.createElement('canvas'), { hidden: true })
  readonly trackedPlanes = new Set<NativePlane>()
  readonly trackedMeshes = new Set<NativeMesh>()
  private planes: LocalPlane[] = []
  private meshes = new Map<MeshBlock, NativeMesh>()

  constructor(_device: XRDevice) {
    const sync = (s: TrackingState, prev?: TrackingState) => {
      if (s.planes !== prev?.planes) this.syncPlanes(s.planes)
      if (s.meshBlocks !== prev?.meshBlocks) this.syncMeshes(s.meshBlocks)
    }
    sync(useTrackingStore.getState())
    useTrackingStore.subscribe(sync)
  }

  private syncPlanes(planes: WorldPlane[]) {
    this.planes = planes.filter((p) => p.border.length >= 3).map(toLocalPlane)
    this.trackedPlanes.clear()
    this.planes.forEach((p) => this.trackedPlanes.add(p.native))
  }

  /** Unchanged blocks keep their NativeMesh, so apps see only changed meshes as new. */
  private syncMeshes(blocks: Map<string, MeshBlock>) {
    const current = new Set(blocks.values())
    for (const [block, native] of this.meshes) {
      if (!current.has(block)) {
        this.meshes.delete(block)
        this.trackedMeshes.delete(native)
      }
    }
    for (const block of current) {
      if (this.meshes.has(block)) continue
      const native = new NativeMesh(IDENTITY, block.positions, block.indices, XRSemanticLabels.GlobalMesh)
      this.meshes.set(block, native)
      this.trackedMeshes.add(native)
    }
  }

  /** Ray (origin, -Z of `rayMatrix`) against the planes; hits face +Y along the normal. */
  computeHitTestResults(rayMatrix: mat4): mat4[] {
    const m = new THREE.Matrix4().fromArray(rayMatrix)
    const origin = new THREE.Vector3().setFromMatrixPosition(m)
    const direction = new THREE.Vector3(0, 0, -1).transformDirection(m)
    return this.planes
      .flatMap((p) => {
        const denom = p.normal.dot(direction)
        if (Math.abs(denom) < 1e-6) return []
        const t = p.normal.dot(p.center.clone().sub(origin)) / denom
        if (t <= 0) return []
        const point = origin.clone().addScaledVector(direction, t)
        const local = point.clone().sub(p.center).applyQuaternion(p.inverse)
        return inside(new THREE.Vector2(local.x, local.z), p.polygon) ? [{ t, point, rotation: p.rotation }] : []
      })
      .sort((a, b) => a.t - b.t)
      .map(({ point, rotation }) => new Float32Array(new THREE.Matrix4().compose(point, rotation, new THREE.Vector3(1, 1, 1)).elements) as mat4)
  }

  render() {}
  loadEnvironment() {}
  async loadDefaultEnvironment() {}
  deleteAll() {}
  computeDepthBuffer() {
    return null
  }
}
