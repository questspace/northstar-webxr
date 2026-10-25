/**
 * The Seeing Spaces workbench: a simulated balancing robot on a bench with the
 * three kinds of seeing from Bret Victor's essay around it as hand-operated
 * instruments — inside (every variable, where it lives), across time (scrub,
 * pin, compare) and across possibilities (a gain sweep you can touch). Each
 * instrument is a module fed from the same frame record and history; the bench
 * itself can be grabbed and moved like any object.
 */
import * as THREE from 'three'
import type { XRApp } from '../xr/app'
import { createMenu, createSlider, createLabel, type Menu } from '../xr/ui'
import { createSim, DEFAULT_PARAMS, DT, PUSH_IMPULSE, type Frame, type Params } from './sim'
import { createHistory } from './history'
import { createRobot } from './instruments/robot'
import { createInside } from './instruments/inside'
import { createTimeline } from './instruments/time'
import { createPossibilities } from './instruments/possibilities'

const SPAN = 10
const HISTORY_SECONDS = 12
const MAX_STEPS_PER_FRAME = 40

export interface WorkbenchOptions {
  origin?: THREE.Vector3Like
}

export interface Workbench {
  group: THREE.Group
  menu: Menu
  params: Params
  update: (dt: number, camera: THREE.Camera) => void
  /** Actions, also bound to keys by the page. */
  pause: (on?: boolean) => void
  push: (direction: 1 | -1) => void
  pin: () => void
  reset: () => void
  scrub: (offset: number) => void
  readonly paused: boolean
  readonly shown: Frame
}

export const createWorkbench = (app: XRApp, { origin = { x: 0, y: 0.85, z: -0.75 } }: WorkbenchOptions = {}): Workbench => {
  const { interaction } = app
  const group = new THREE.Group()
  group.position.copy(origin)

  const params: Params = { ...DEFAULT_PARAMS }
  const sim = createSim(params)
  const history = createHistory(Math.round(HISTORY_SECONDS / DT))
  let paused = false
  let offset = 0
  let pending = 0
  let lastPush = PUSH_IMPULSE
  let pinned: Frame | null = null
  let accumulator = 0
  let sweepDirty = true
  let sweepTimer = 0

  // Bench
  const table = new THREE.Mesh(
    new THREE.BoxGeometry(0.9, 0.01, 0.5),
    new THREE.MeshStandardMaterial({ color: 0x1a2634, transparent: true, opacity: 0.55, roughness: 0.9 }),
  )
  table.position.y = -0.005
  const track = new THREE.Mesh(new THREE.BoxGeometry(0.62, 0.002, 0.14), new THREE.MeshBasicMaterial({ color: 0x24344a }))
  track.position.y = 0.001
  const handle = new THREE.Mesh(new THREE.BoxGeometry(0.2, 0.014, 0.014), new THREE.MeshStandardMaterial({ color: 0x4a5568 }))
  handle.position.set(0, 0.007, 0.26)
  interaction.add({
    object: handle,
    radius: 0.03,
    grab: { target: group },
    onHover: (h) => { (handle.material as THREE.MeshStandardMaterial).color.set(h ? 0x90cdf4 : 0x4a5568) },
  })
  const title = createLabel('Seeing Spaces workbench · self-balancing robot', { size: 0.028, color: '#9fd3ff' })
  title.group.position.set(0, 0.72, -0.22)
  group.add(table, track, handle, title.group)

  // The thing, and its ghost for pinned moments
  const robot = createRobot()
  const ghost = createRobot({ ghost: true })
  ghost.group.position.z = -0.17
  ghost.group.visible = false
  group.add(robot.group, ghost.group)
  robot.pads.forEach((pad, i) =>
    interaction.add({ object: pad, press: { size: [0.09, 0.16], travel: 0.008, onPress: () => push(i === 0 ? 1 : -1) } }),
  )

  // Instruments
  const inside = createInside(robot, history)
  const timeline = createTimeline(interaction, history, SPAN, (o) => scrub(o))
  timeline.group.position.set(0, 0.52, -0.22)
  timeline.group.rotation.x = 0.22
  const possibilities = createPossibilities(interaction, (kp, kd) => setGains({ kp, kd }))
  possibilities.group.position.set(0.74, 0.3, -0.1)
  possibilities.group.rotation.y = -0.55
  group.add(inside.group, timeline.group, possibilities.group)

  const sliders = (['kp', 'kd', 'ki'] as const).map((key, i) => {
    const range = { kp: [0.3, 20], kd: [0, 1.5], ki: [0, 6] }[key] as [number, number]
    const slider = createSlider(interaction, {
      label: key[0].toUpperCase() + key[1],
      min: range[0],
      max: range[1],
      value: params[key],
      length: 0.2,
      onChange: (v) => setGains({ [key]: v }),
    })
    slider.group.position.set((i - 1) * 0.28, 0.012, 0.19)
    slider.group.rotation.x = -Math.PI / 2 + 0.75
    group.add(slider.group)
    return { key, slider }
  })

  const setGains = (gains: Partial<Params>) => {
    Object.assign(params, gains)
    for (const { key, slider } of sliders) if (key in gains) slider.set(params[key])
    sweepDirty = true
  }

  // Actions
  const pause = (on = !paused) => {
    paused = on
    if (!paused) {
      offset = 0
      timeline.setOffset(0)
    }
  }
  const push = (direction: 1 | -1) => {
    pending = direction * PUSH_IMPULSE
    lastPush = pending
    sweepDirty = true
    if (paused) pause(false)
  }
  const scrub = (o: number) => {
    offset = Math.min(0, o)
    if (offset > -0.05) pause(false)
    else paused = true
  }
  const pin = () => {
    pinned = pinned ? null : shown()
    ghost.group.visible = pinned !== null
    if (pinned) ghost.set(pinned)
  }
  const reset = () => {
    sim.reset()
    history.clear()
    pinned = null
    ghost.group.visible = false
    pause(false)
  }
  const shown = (): Frame => {
    const last = history.last ?? sim.frame
    return paused && offset < 0 ? history.at(last.t + offset) ?? last : last
  }

  const menu = createMenu(interaction, {
    title: 'Workbench',
    items: [
      { label: 'Pause', onPress: () => pause(), toggled: () => paused },
      { label: 'Push →', onPress: () => push(1) },
      { label: 'Push ←', onPress: () => push(-1) },
      { label: 'Pin', onPress: pin, toggled: () => pinned !== null },
      { label: 'Noise', onPress: () => { params.noise = !params.noise; sweepDirty = true }, toggled: () => params.noise },
      { label: 'Sweep', onPress: () => { possibilities.group.visible = !possibilities.group.visible }, toggled: () => possibilities.group.visible },
      { label: 'Reset', onPress: reset },
      { label: 'Occluders', onPress: () => { app.occluders.enabled = !app.occluders.enabled }, toggled: () => app.occluders.enabled },
    ],
  })
  menu.group.position.set(-0.74, 0.32, -0.1)
  menu.group.rotation.y = 0.55
  group.add(menu.group)

  const update = (dt: number, camera: THREE.Camera) => {
    if (!paused) {
      accumulator = Math.min(accumulator + dt, MAX_STEPS_PER_FRAME * DT)
      while (accumulator >= DT) {
        history.push(sim.step(pending))
        pending = 0
        accumulator -= DT
      }
    }
    const frame = shown()
    robot.set(frame)
    inside.update(frame)
    const now = (history.last ?? sim.frame).t
    timeline.update({ now, viewT: frame.t, pinnedT: pinned?.t ?? null, live: !paused })
    possibilities.update(params)
    menu.update(camera)

    // Sweep after the gains settle for a moment (synchronous: 25 short sims).
    sweepTimer = sweepDirty ? sweepTimer + dt : 0
    if (sweepDirty && sweepTimer > 0.3) {
      sweepDirty = false
      possibilities.compute(params, lastPush)
    }
  }

  return {
    group,
    menu,
    params,
    update,
    pause,
    push,
    pin,
    reset,
    scrub,
    get paused() { return paused },
    get shown() { return shown() },
  }
}
