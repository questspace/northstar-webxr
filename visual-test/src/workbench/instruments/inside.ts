/**
 * Seeing inside: every sensor reading and controller variable laid out around
 * the robot, each with a live value, a signed bar, a two-second sparkline and a
 * leader line to the part of the robot it belongs to.
 */
import * as THREE from 'three'
import type { Frame } from '../sim'
import type { History } from '../history'
import { createCanvasPlane, panel, text, traces, bar, COLORS } from '../chart'
import type { Robot, PartName } from './robot'

interface Readout {
  name: string
  unit: string
  range: [number, number]
  value: (f: Frame) => number
  part: PartName
  side: 'left' | 'right'
  format?: (v: number) => string
}

const deg = (rad: number) => (rad * 180) / Math.PI
const fixed = (digits: number) => (v: number) => v.toFixed(digits)

const READOUTS: Readout[] = [
  { name: 'gyro', unit: 'rad/s', range: [-6, 6], value: (f) => f.sensors.gyro, part: 'imu', side: 'left', format: fixed(2) },
  { name: 'accel tilt', unit: '°', range: [-45, 45], value: (f) => deg(f.sensors.accelTilt), part: 'imu', side: 'left', format: fixed(1) },
  { name: 'wheel speed', unit: 'rad/s', range: [-30, 30], value: (f) => f.sensors.wheelSpeed, part: 'wheel', side: 'left', format: fixed(1) },
  { name: 'motor current', unit: 'A', range: [0, 3], value: (f) => f.sensors.current, part: 'motor', side: 'left', format: fixed(2) },
  { name: 'battery', unit: 'V', range: [6, 8.4], value: (f) => f.sensors.battery, part: 'battery', side: 'left', format: fixed(2) },
  { name: 'tilt estimate', unit: '°', range: [-45, 45], value: (f) => deg(f.internals.estimate), part: 'board', side: 'right', format: fixed(1) },
  { name: 'setpoint', unit: '°', range: [-10, 10], value: (f) => deg(f.internals.setpoint), part: 'top', side: 'right', format: fixed(1) },
  { name: 'P term', unit: '', range: [-2, 2], value: (f) => f.internals.p, part: 'board', side: 'right', format: fixed(2) },
  { name: 'I term', unit: '', range: [-1, 1], value: (f) => f.internals.i, part: 'board', side: 'right', format: fixed(2) },
  { name: 'D term', unit: '', range: [-2, 2], value: (f) => f.internals.d, part: 'board', side: 'right', format: fixed(2) },
  { name: 'command', unit: '', range: [-1, 1], value: (f) => f.internals.command, part: 'motor', side: 'right', format: fixed(2) },
  { name: 'torque', unit: 'N·m', range: [-0.35, 0.35], value: (f) => f.internals.torque, part: 'axle', side: 'right', format: fixed(3) },
]

const WIDTH = 0.17
const HEIGHT = 0.034
const COLUMN_X = 0.42
const TOP_Y = 0.42
const ROW = 0.04
const SPARK_SECONDS = 2
const SIDE_COLOR = { left: '#63d2ff', right: '#ffb347' }

export interface Inside {
  group: THREE.Group
  update: (shown: Frame) => void
}

export const createInside = (robot: Robot, history: History): Inside => {
  const group = new THREE.Group()
  const rows = { left: 0, right: 0 }
  const items = READOUTS.map((readout) => {
    const plane = createCanvasPlane(WIDTH, HEIGHT, 512)
    const sign = readout.side === 'left' ? -1 : 1
    const row = rows[readout.side]++
    plane.mesh.position.set(sign * COLUMN_X, TOP_Y - row * ROW, 0)
    const leader = new THREE.Line(
      new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(), new THREE.Vector3()]),
      new THREE.LineBasicMaterial({ color: SIDE_COLOR[readout.side], transparent: true, opacity: 0.45 }),
    )
    group.add(plane.mesh, leader)
    return { readout, plane, leader, sign }
  })

  const world = new THREE.Vector3()
  let tick = 0
  const draw = (shown: Frame) => {
    for (const { readout, plane } of items) {
      const { ctx, width, height } = plane
      const value = readout.value(shown)
      const color = SIDE_COLOR[readout.side]
      ctx.clearRect(0, 0, width, height)
      panel(ctx, { x: 0, y: 0, w: width, h: height }, 14)
      text(ctx, readout.name, 14, 28, 26, COLORS.dim)
      text(ctx, `${(readout.format ?? fixed(2))(value)} ${readout.unit}`, 236, 30, 34, COLORS.text, 'right', '600')
      bar(ctx, { x: 14, y: 72, w: 222, h: 14 }, value, readout.range, color)
      const spark = history.range(shown.t - SPARK_SECONDS, shown.t, 120).map(readout.value)
      traces(ctx, { x: 262, y: 12, w: width - 276, h: height - 24 }, [{ values: spark, color, width: 3 }], readout.range)
      plane.commit()
    }
  }

  return {
    group,
    update: (shown) => {
      // Leader lines every frame (the robot moves), canvases at half rate.
      for (const { readout, plane, leader, sign } of items) {
        robot.parts[readout.part].getWorldPosition(world)
        group.worldToLocal(world)
        const positions = leader.geometry.attributes.position as THREE.BufferAttribute
        positions.setXYZ(0, plane.mesh.position.x - sign * (WIDTH / 2), plane.mesh.position.y, 0)
        positions.setXYZ(1, world.x, world.y, world.z)
        positions.needsUpdate = true
      }
      if (tick++ % 2 === 0) draw(shown)
    },
  }
}
