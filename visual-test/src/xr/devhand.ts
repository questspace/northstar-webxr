/**
 * A synthetic hand for developing hand interactions without the headset: the
 * right hand (left with Shift) hovers where the mouse points, 0.45 m ahead of
 * the head by default, the wheel moves it nearer or farther, holding the mouse
 * button pinches. Goes through the same store → XRHand path as the Leap, so
 * the interaction code under test is the real one. Scriptable through
 * `window.devHand.set(...)`.
 */
import * as THREE from 'three'
import type { NorthStarCalibration } from '../lib/calibration'
import { headToLeap } from '../lib/transforms'
import { useTrackingStore, type HandData, type Vec3 } from '../stores/tracking'

export interface DevHandPose {
  /** Index fingertip in the head frame (meters: X right, Y up, -Z forward) */
  tip: THREE.Vector3Like
  pinch: boolean
}

export interface DevHand {
  set: (hand: 'left' | 'right', pose: DevHandPose) => void
  /** Index fingertip in the head frame for mouse NDC coordinates and a depth */
  fromPointer: (ndcX: number, ndcY: number, depth: number) => THREE.Vector3
}

const FINGER_X = [0, 0, 0.019, 0.038, 0.056] // thumb (set separately), index .. pinky
const JOINT_Z = [0.095, 0.065, 0.038, 0.017, 0] // carp, mcp, pip, dip, tip along the finger

/** An open right hand in the head frame, index tip at `tip`; mirrored in X for the left. */
const shape = (hand: 'left' | 'right', tip: THREE.Vector3Like, pinch: boolean) => {
  const sign = hand === 'right' ? 1 : -1
  const at = (x: number, y: number, z: number) => new THREE.Vector3(tip.x + sign * x, tip.y + y, tip.z + z)
  const fingers = FINGER_X.map((x, finger) =>
    JOINT_Z.map((z, joint) => (finger === 0 ? thumb(joint) : at(x, -0.012 + joint * 0.003, z))),
  )
  function thumb(joint: number) {
    // From the side of the palm to the index tip when pinching, 4 cm beside it otherwise.
    const spread = pinch ? 0.004 : 0.04
    const t = joint / 4
    return at(-(0.045 + (spread - 0.045) * t), -0.02 + t * 0.012, 0.085 - t * 0.08)
  }
  return {
    fingers,
    palm: at(0.025, -0.02, 0.075),
    wrist: at(0.02, -0.025, 0.12),
    normal: new THREE.Vector3(0, -1, 0),
    direction: new THREE.Vector3(0, 0, -1),
  }
}

export const installDevHand = (calibration: NorthStarCalibration): DevHand => {
  const toLeap = headToLeap(calibration)
  const mm = (p: THREE.Vector3) => toLeap.point(p).toArray() as Vec3
  const dir = (d: THREE.Vector3) => toLeap.direction(d).toArray() as Vec3
  const hands: Record<'left' | 'right', HandData | null> = { left: null, right: null }

  const set: DevHand['set'] = (hand, { tip, pinch }) => {
    const s = shape(hand, tip, pinch)
    hands[hand] = {
      id: hand === 'left' ? 1 : 2,
      type: hand,
      palmPosition: mm(s.palm),
      palmNormal: dir(s.normal),
      direction: dir(s.direction),
      wrist: mm(s.wrist),
      pinchStrength: pinch ? 1 : 0,
      fingers: s.fingers.map((joints) => joints.map(mm)),
      fingerWidth: [18, 16, 16, 15, 13],
    }
    useTrackingStore.getState().setHands(hands.left, hands.right)
  }
  const fromPointer: DevHand['fromPointer'] = (x, y, depth) => new THREE.Vector3(x * depth * 0.55, y * depth * 0.35, -depth)

  let depth = 0.45
  let down = false
  let ndc = { x: 0, y: 0 }
  let shift = false
  const drive = () => set(shift ? 'left' : 'right', { tip: fromPointer(ndc.x, ndc.y, depth), pinch: down })
  window.addEventListener('pointermove', (e) => {
    ndc = { x: (e.clientX / window.innerWidth) * 2 - 1, y: 1 - (e.clientY / window.innerHeight) * 2 }
    drive()
  })
  window.addEventListener('pointerdown', (e) => { down = e.button === 0; drive() })
  window.addEventListener('pointerup', () => { down = false; drive() })
  window.addEventListener('wheel', (e) => { depth = THREE.MathUtils.clamp(depth - e.deltaY * 0.0003, 0.15, 1.2); drive() })
  window.addEventListener('keydown', (e) => { if (e.key === 'Shift') shift = true })
  window.addEventListener('keyup', (e) => { if (e.key === 'Shift') shift = false })
  useTrackingStore.getState().setLeapConnected(true)
  useTrackingStore.getState().setLeapStatus('Dev hand (mouse)')

  const devHand = { set, fromPointer }
  ;(window as unknown as { devHand: DevHand }).devHand = devHand
  return devHand
}
