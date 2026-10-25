/** The robot on the bench, with named parts that the readouts' leader lines attach to. */
import * as THREE from 'three'
import { WHEEL_RADIUS, type Frame } from '../sim'

export type PartName = 'imu' | 'wheel' | 'battery' | 'motor' | 'top' | 'axle' | 'board'

export interface Robot {
  group: THREE.Group
  parts: Record<PartName, THREE.Object3D>
  /** Invisible faces on either side of the body for a fingertip push: [left, right] */
  pads: THREE.Mesh[]
  set: (frame: Frame) => void
}

const box = (w: number, h: number, d: number, material: THREE.Material, x = 0, y = 0, z = 0) => {
  const mesh = new THREE.Mesh(new THREE.BoxGeometry(w, h, d), material)
  mesh.position.set(x, y, z)
  return mesh
}

export const createRobot = ({ ghost = false } = {}): Robot => {
  const material = (color: number) =>
    ghost
      ? new THREE.MeshBasicMaterial({ color: 0x7ef0a0, transparent: true, opacity: 0.3, depthWrite: false })
      : new THREE.MeshStandardMaterial({ color, roughness: 0.6, metalness: 0.1 })

  const group = new THREE.Group()
  const axle = new THREE.Group()
  axle.position.y = WHEEL_RADIUS
  group.add(axle)

  const tyre = material(0x1c1c22)
  const wheels = [-0.055, 0.055].map((z) => {
    const wheel = new THREE.Mesh(new THREE.CylinderGeometry(WHEEL_RADIUS, WHEEL_RADIUS, 0.018, 28).rotateX(Math.PI / 2), tyre)
    wheel.position.z = z
    const hub = new THREE.Mesh(new THREE.CylinderGeometry(0.012, 0.012, 0.02, 12).rotateX(Math.PI / 2), material(0xa0aec0))
    wheel.add(hub)
    axle.add(wheel)
    return wheel
  })

  const body = new THREE.Group()
  axle.add(body)
  const chassis = box(0.07, 0.012, 0.1, material(0x4a5568), 0, 0, 0)
  const motors = [-0.04, 0.04].map((z) => box(0.02, 0.022, 0.03, material(0x9aa5b1), 0, 0.012, z))
  const battery = box(0.055, 0.03, 0.07, material(0x2b6cb0), 0, 0.03, 0)
  const standoffs = [-0.028, 0.028].flatMap((x) => [-0.04, 0.04].map((z) => {
    const rod = new THREE.Mesh(new THREE.CylinderGeometry(0.0025, 0.0025, 0.14, 8), material(0xcbd5e0))
    rod.position.set(x, 0.12, z)
    return rod
  }))
  const board = box(0.062, 0.004, 0.084, material(0x1f7a3a), 0, 0.19, 0)
  const imu = box(0.012, 0.004, 0.012, material(0x111111), 0.015, 0.195, 0.02)
  const top = box(0.07, 0.06, 0.06, material(0xd97706), 0, 0.235, 0)
  const pads = [-1, 1].map((side) => {
    const pad = box(0.09, 0.16, 0.01, new THREE.MeshBasicMaterial({ visible: false }), side * 0.045, 0.17, 0)
    pad.rotation.y = (side * Math.PI) / 2
    return pad
  })
  body.add(chassis, ...motors, battery, ...standoffs, board, imu, top, ...pads)

  const parts: Record<PartName, THREE.Object3D> = { imu, wheel: wheels[1], battery, motor: motors[1], top, axle, board }
  let lastT: number | null = null
  return {
    group,
    parts,
    pads,
    set: (frame) => {
      const { state } = frame
      group.position.x = state.x
      body.rotation.z = -state.theta
      const dt = lastT === null ? 0 : frame.t - lastT
      lastT = frame.t
      for (const wheel of wheels) wheel.rotation.z -= (state.v / WHEEL_RADIUS) * dt
    },
  }
}
