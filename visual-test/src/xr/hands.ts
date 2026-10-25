/**
 * Hand occluders for optical see-through: a depth-only mesh built from the WebXR
 * hand joints every frame (spheres at the joints, capsules along the bones, a
 * palm ellipsoid). It writes no colour, so holograms behind the real hand are
 * cut out and the hand reads as being in front of them. `visible: true` draws
 * it translucent instead, for desktop previews.
 */
import * as THREE from 'three'

const FINGERS = ['thumb', 'index-finger', 'middle-finger', 'ring-finger', 'pinky-finger'] as const
const SEGMENTS = ['metacarpal', 'phalanx-proximal', 'phalanx-intermediate', 'phalanx-distal', 'tip'] as const

/** Bone chains from the wrist out to each tip (the thumb has no intermediate phalanx). */
const CHAINS: XRHandJoint[][] = FINGERS.map((finger) => [
  'wrist',
  ...SEGMENTS.filter((s) => !(finger === 'thumb' && s === 'phalanx-intermediate')).map((s) => `${finger}-${s}` as XRHandJoint),
])
const BONES = CHAINS.flatMap((chain) => chain.slice(1).map((joint, i) => [chain[i], joint] as const))
const JOINTS = [...new Set(CHAINS.flat())]
const RADIUS_SCALE = 1.25

export interface HandOccluder {
  group: THREE.Group
  update: () => void
}

export const createHandOccluder = (hand: THREE.XRHandSpace, { visible = false } = {}): HandOccluder => {
  const material = visible
    ? new THREE.MeshStandardMaterial({ color: 0x334455, transparent: true, opacity: 0.55, roughness: 0.8 })
    : new THREE.MeshBasicMaterial({ colorWrite: false })
  const joints = new THREE.InstancedMesh(new THREE.SphereGeometry(1, 10, 8), material, JOINTS.length)
  const bones = new THREE.InstancedMesh(new THREE.CylinderGeometry(1, 1, 1, 10, 1, true).rotateX(Math.PI / 2), material, BONES.length)
  const palm = new THREE.Mesh(new THREE.SphereGeometry(1, 16, 12), material)
  const group = new THREE.Group()
  group.add(joints, bones, palm)
  // Depth first, so everything drawn afterwards is tested against the hand.
  for (const mesh of [joints, bones, palm]) {
    mesh.renderOrder = -1
    mesh.frustumCulled = false
  }

  const p = new THREE.Vector3()
  const t = new THREE.Vector3()
  const s = new THREE.Vector3()
  const q = new THREE.Quaternion()
  const matrix = new THREE.Matrix4()
  const up = new THREE.Vector3(0, 0, 1)
  const radius = (name: XRHandJoint) => (hand.joints[name]?.jointRadius ?? 0.008) * RADIUS_SCALE
  const position = (name: XRHandJoint, out: THREE.Vector3) => hand.joints[name]!.getWorldPosition(out)

  const update = () => {
    const tracked = JOINTS.every((name) => hand.joints[name]?.visible)
    group.visible = tracked
    if (!tracked) return

    JOINTS.forEach((name, i) => {
      const r = radius(name)
      joints.setMatrixAt(i, matrix.compose(position(name, p), q.identity(), s.set(r, r, r)))
    })
    BONES.forEach(([from, to], i) => {
      position(from, p)
      position(to, t)
      const length = p.distanceTo(t)
      const r = Math.min(radius(from), radius(to))
      q.setFromUnitVectors(up, t.sub(p).normalize())
      bones.setMatrixAt(i, matrix.compose(p.addScaledVector(t, length / 2), q, s.set(r, r, length)))
    })
    joints.instanceMatrix.needsUpdate = true
    bones.instanceMatrix.needsUpdate = true

    // Palm: an ellipsoid between the wrist and the middle finger's knuckle, flattened along the palm normal.
    position('wrist', p)
    position('middle-finger-phalanx-proximal', t)
    const forward = t.clone().sub(p)
    const across = position('index-finger-phalanx-proximal', s).sub(position('pinky-finger-phalanx-proximal', new THREE.Vector3()))
    const normal = new THREE.Vector3().crossVectors(forward, across).normalize()
    const basis = new THREE.Matrix4().makeBasis(across.clone().normalize(), forward.clone().normalize(), normal)
    palm.position.copy(p).addScaledVector(forward, 0.5)
    palm.quaternion.setFromRotationMatrix(basis)
    palm.scale.set(across.length() * 0.6, forward.length() * 0.6, 0.014)
  }

  return { group, update }
}

/** Occluders for both hands of a renderer, as one object with one `update`. */
export const createHandOccluders = (renderer: THREE.WebGLRenderer, scene: THREE.Scene, options?: { visible?: boolean }) => {
  const occluders = [0, 1].map((i) => createHandOccluder(renderer.xr.getHand(i), options))
  for (const o of occluders) scene.add(o.group)
  let enabled = true
  return {
    update: () => occluders.forEach((o) => {
      o.update()
      o.group.visible &&= enabled
    }),
    set enabled(on: boolean) { enabled = on },
    get enabled() { return enabled },
  }
}
