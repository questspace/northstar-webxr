/**
 * Hand interaction on top of WebXR hand tracking as three.js exposes it
 * (`renderer.xr.getHand(i).joints`): pinch to grab (one hand moves and turns,
 * two hands scale), fingertip presses for buttons, path-constrained grabs for
 * sliders, hover. Runtime-agnostic: any device that reports XRHand joints.
 */
import * as THREE from 'three'

export type Handedness = 'left' | 'right' | 'none'

export interface HandState {
  readonly space: THREE.XRHandSpace
  handedness: Handedness
  tracked: boolean
  pinching: boolean
  /** World space: index tip, thumb tip, pinch point (between them), palm centre, palm normal (out of the palm). */
  readonly index: THREE.Vector3
  readonly thumb: THREE.Vector3
  readonly pinch: THREE.Vector3
  readonly palm: THREE.Vector3
  readonly palmNormal: THREE.Vector3
  /** Wrist orientation: the frame a grabbed object follows. */
  readonly wrist: THREE.Quaternion
  grab: Grab | null
  pressing: Interactable | null
}

export interface GrabOptions {
  /** Object moved by the grab when it is not the proximity object itself (a menu by its handle). */
  target?: THREE.Object3D
  /** Keeps the target's local position on a path (sliders); rotation and scale stay. */
  constrain?: (local: THREE.Vector3) => void
  onStart?: (hand: HandState) => void
  onMove?: (hand: HandState) => void
  onEnd?: (hand: HandState) => void
}

export interface PressOptions {
  /** Local-space face (width, height) centred on the object's origin; the finger comes from +Z. */
  size: [number, number]
  /** Finger travel from the face (local z = 0) to the click, in meters. */
  travel?: number
  onPress: (local: THREE.Vector3, hand: HandState) => void
  /** Push depth 0..1 while a finger is on the face, for the cap animation. */
  onPush?: (amount: number) => void
}

export interface Interactable {
  object: THREE.Object3D
  /** Reach around the object's origin for grabbing; default: its bounding sphere. */
  radius?: number
  grab?: GrabOptions | true
  press?: PressOptions
  onHover?: (hovered: boolean) => void
  hovered?: boolean
}

interface Grab {
  item: Interactable
  target: THREE.Object3D
  options: GrabOptions
  /** Grab frame⁻¹ · target world matrix (single hand) */
  offset: THREE.Matrix4
  /** Shared while two hands hold the same target */
  pair?: { d0: THREE.Vector3; m0: THREE.Vector3; world0: THREE.Matrix4 }
}

export interface Interaction {
  readonly hands: HandState[]
  readonly items: ReadonlySet<Interactable>
  add: (item: Interactable) => Interactable
  remove: (item: Interactable) => void
  /** Call once per frame before rendering. */
  update: () => void
  /** The hand holding `object`, if any. */
  holder: (object: THREE.Object3D) => HandState | undefined
}

const PINCH_START = 0.02
const PINCH_END = 0.035
const HOVER_REACH = 0.03
const PRESS_HOVER_DEPTH = 0.05

const v = { a: new THREE.Vector3(), b: new THREE.Vector3(), c: new THREE.Vector3() }
const m = { a: new THREE.Matrix4(), b: new THREE.Matrix4() }
const q = new THREE.Quaternion()

const boundingRadius = (object: THREE.Object3D) => {
  const box = new THREE.Box3().setFromObject(object)
  return box.isEmpty() ? 0.05 : box.getBoundingSphere(new THREE.Sphere()).radius
}

/** Sets an object's world matrix, keeping it a child of its parent. */
const setWorldMatrix = (object: THREE.Object3D, world: THREE.Matrix4) => {
  const parentInverse = object.parent ? m.b.copy(object.parent.matrixWorld).invert() : m.b.identity()
  m.a.multiplyMatrices(parentInverse, world).decompose(object.position, object.quaternion, object.scale)
}

const grabFrame = (hand: HandState, out: THREE.Matrix4) => out.compose(hand.pinch, hand.wrist, v.c.set(1, 1, 1))

const createHand = (space: THREE.XRHandSpace): HandState => {
  const hand: HandState = {
    space,
    handedness: 'none',
    tracked: false,
    pinching: false,
    index: new THREE.Vector3(),
    thumb: new THREE.Vector3(),
    pinch: new THREE.Vector3(),
    palm: new THREE.Vector3(),
    palmNormal: new THREE.Vector3(),
    wrist: new THREE.Quaternion(),
    grab: null,
    pressing: null,
  }
  space.addEventListener('connected', (e) => { hand.handedness = (e.data?.handedness ?? 'none') as Handedness })
  space.addEventListener('disconnected', () => { hand.tracked = false })
  return hand
}

/** Reads this frame's joints into world-space hand state. */
const readHand = (hand: HandState) => {
  const j = hand.space.joints
  const index = j['index-finger-tip']
  const thumb = j['thumb-tip']
  const wrist = j['wrist']
  const indexMcp = j['index-finger-phalanx-proximal']
  const pinkyMcp = j['pinky-finger-phalanx-proximal']
  const middle = j['middle-finger-phalanx-proximal']
  hand.tracked = !!(index?.visible && thumb?.visible && wrist?.visible && indexMcp && pinkyMcp && middle)
  if (!hand.tracked) {
    hand.pinching = false
    return
  }
  index!.getWorldPosition(hand.index)
  thumb!.getWorldPosition(hand.thumb)
  hand.pinch.addVectors(hand.index, hand.thumb).multiplyScalar(0.5)
  wrist!.getWorldQuaternion(hand.wrist)
  wrist!.getWorldPosition(v.a)
  middle!.getWorldPosition(hand.palm).add(v.a).multiplyScalar(0.5)
  // Palm normal from the hand's own geometry, so it does not depend on the runtime's joint axes.
  indexMcp!.getWorldPosition(v.b).sub(v.a)
  pinkyMcp!.getWorldPosition(v.c).sub(v.a)
  hand.palmNormal.crossVectors(v.b, v.c).normalize()
  if (hand.handedness === 'left') hand.palmNormal.negate()

  const distance = hand.index.distanceTo(hand.thumb)
  hand.pinching = hand.pinching ? distance < PINCH_END : distance < PINCH_START
}

export const createInteraction = (renderer: THREE.WebGLRenderer, scene: THREE.Scene): Interaction => {
  const items = new Set<Interactable>()
  const pushes = new Map<Interactable, { amount: number; armed: boolean }>()
  const hands = [0, 1].map((i) => {
    const space = renderer.xr.getHand(i)
    scene.add(space)
    return createHand(space)
  })
  const cursors = hands.map(() => {
    const material = new THREE.MeshBasicMaterial({ color: 0x8fe3ff, depthTest: false, transparent: true, opacity: 0.9 })
    const cursor = new THREE.Mesh(new THREE.SphereGeometry(0.004, 12, 12), material)
    cursor.renderOrder = 1000
    cursor.visible = false
    scene.add(cursor)
    return cursor
  })

  const grabOptions = (item: Interactable): GrabOptions => (item.grab === true ? {} : item.grab!)
  const grabTarget = (item: Interactable) => grabOptions(item).target ?? item.object
  const holderOf = (target: THREE.Object3D, except?: HandState) => hands.find((h) => h !== except && h.grab?.target === target)

  const nearestGrabbable = (point: THREE.Vector3) => {
    let best: Interactable | undefined
    let bestDistance = HOVER_REACH
    for (const item of items) {
      if (!item.grab) continue
      const distance = item.object.getWorldPosition(v.a).distanceTo(point) - item.radius!
      if (distance < bestDistance) {
        best = item
        bestDistance = distance
      }
    }
    return best
  }

  const startGrab = (hand: HandState, item: Interactable) => {
    const target = grabTarget(item)
    const options = grabOptions(item)
    const grab: Grab = { item, target, options, offset: new THREE.Matrix4() }
    grab.offset.multiplyMatrices(grabFrame(hand, m.a).invert(), target.matrixWorld)
    const other = holderOf(target, hand)
    if (other?.grab && !options.constrain) {
      grab.pair = other.grab.pair = {
        d0: hand.pinch.clone().sub(other.pinch),
        m0: hand.pinch.clone().add(other.pinch).multiplyScalar(0.5),
        world0: target.matrixWorld.clone(),
      }
    }
    hand.grab = grab
    options.onStart?.(hand)
  }

  const endGrab = (hand: HandState) => {
    const grab = hand.grab!
    hand.grab = null
    const other = holderOf(grab.target)
    if (other?.grab) {
      // The remaining hand carries on alone from where the object is now.
      other.grab.pair = undefined
      other.grab.offset.multiplyMatrices(grabFrame(other, m.a).invert(), grab.target.matrixWorld)
    }
    grab.options.onEnd?.(hand)
  }

  const applyGrab = (hand: HandState) => {
    const grab = hand.grab!
    const { target, options } = grab
    if (options.constrain) {
      const local = target.parent ? target.parent.worldToLocal(v.a.copy(hand.pinch)) : v.a.copy(hand.pinch)
      options.constrain(local)
      target.position.copy(local)
    } else if (grab.pair) {
      const other = holderOf(target, hand)!
      const { d0, m0, world0 } = grab.pair
      const d = v.a.subVectors(hand.pinch, other.pinch)
      const mid = v.b.addVectors(hand.pinch, other.pinch).multiplyScalar(0.5)
      const scale = d.length() / Math.max(d0.length(), 1e-4)
      q.setFromUnitVectors(v.c.copy(d0).normalize(), d.normalize())
      // T(mid) · R · S · T(-m0) · world0
      m.a.compose(mid, q, v.c.set(scale, scale, scale)).multiply(m.b.makeTranslation(-m0.x, -m0.y, -m0.z)).multiply(world0)
      setWorldMatrix(target, m.a)
    } else {
      setWorldMatrix(target, grabFrame(hand, m.a).multiply(grab.offset))
    }
    target.updateMatrixWorld(true)
    options.onMove?.(hand)
  }

  /** Index fingertip against every button face; returns the one being pushed, if any. */
  const press = (hand: HandState) => {
    let pressing: Interactable | null = null
    for (const item of items) {
      if (!item.press) continue
      const { size, travel = 0.012, onPress } = item.press
      const local = item.object.worldToLocal(v.a.copy(hand.index))
      const onFace = Math.abs(local.x) <= size[0] / 2 && Math.abs(local.y) <= size[1] / 2
      const inRange = local.z < PRESS_HOVER_DEPTH && local.z > -travel - 0.02
      const depth = onFace && inRange ? THREE.MathUtils.clamp(-local.z / travel, 0, 1) : 0
      const state = pushes.get(item) ?? { amount: 0, armed: true }
      pushes.set(item, state)
      state.amount = Math.max(state.amount, depth)
      if (depth > 0) pressing = item
      if (depth >= 1 && state.armed) {
        state.armed = false
        onPress(local.clone(), hand)
      } else if (depth < 0.5) {
        state.armed = true
      }
    }
    return pressing
  }

  const hovered = (item: Interactable) => {
    if (item.press) return (pushes.get(item)?.amount ?? 0) > 0
    const origin = item.object.getWorldPosition(v.a)
    return hands.some((h) => h.tracked && (h.grab?.item === item || (!h.grab && h.pinch.distanceTo(origin) < item.radius! + HOVER_REACH)))
  }

  const update = () => {
    for (const state of pushes.values()) state.amount = 0
    hands.forEach((hand, i) => {
      readHand(hand)
      const cursor = cursors[i]
      cursor.visible = hand.tracked
      if (!hand.tracked) {
        if (hand.grab) endGrab(hand)
        hand.pressing = null
        return
      }
      if (hand.pinching && !hand.grab && !hand.pressing) {
        const item = nearestGrabbable(hand.pinch)
        if (item) startGrab(hand, item)
      } else if (!hand.pinching && hand.grab) {
        endGrab(hand)
      }
      if (hand.grab) applyGrab(hand)
      hand.pressing = hand.grab ? null : press(hand)
      cursor.position.copy(hand.pinching ? hand.pinch : hand.index)
      cursor.scale.setScalar(hand.pinching ? 1.6 : 1)
      ;(cursor.material as THREE.MeshBasicMaterial).color.set(hand.pinching ? 0xffb347 : hand.pressing ? 0xffffff : 0x8fe3ff)
    })
    for (const [item, state] of pushes) item.press?.onPush?.(state.amount)
    for (const item of items) {
      const now = hovered(item)
      if (now !== item.hovered) {
        item.hovered = now
        item.onHover?.(now)
      }
    }
  }

  return {
    hands,
    items,
    add: (item) => {
      item.radius ??= boundingRadius(item.object)
      items.add(item)
      return item
    },
    remove: (item) => {
      items.delete(item)
      pushes.delete(item)
      for (const hand of hands) if (hand.grab?.item === item) endGrab(hand)
    },
    update,
    holder: (object) => hands.find((h) => h.grab?.target === object || h.grab?.item.object === object),
  }
}
