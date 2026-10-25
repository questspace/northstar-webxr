/**
 * Seeing across time: the last ten seconds of the robot as traces, with push
 * marks and fallen spans, a playhead, a pinned moment to compare against, and a
 * scrub knob that pauses the world and moves the robot back through its history.
 */
import * as THREE from 'three'
import type { Interaction } from '../../xr/interaction'
import { createSlider } from '../../xr/ui'
import type { History } from '../history'
import { createCanvasPlane, panel, text, traces, COLORS } from '../chart'

export interface TimeView {
  /** Latest recorded time */
  now: number
  /** Time shown on the robot and readouts */
  viewT: number
  pinnedT: number | null
  live: boolean
}

export interface Timeline {
  group: THREE.Group
  update: (view: TimeView) => void
  /** Moves the scrub knob (0 = live) without firing onScrub. */
  setOffset: (offset: number) => void
}

const WIDTH = 0.64
const HEIGHT = 0.18
const PX = 1280
const PLOT = { x: 24, y: 70, w: PX - 48, h: 0 }
const LEAN_RANGE: [number, number] = [-1.1, 1.1]

export const createTimeline = (interaction: Interaction, history: History, span: number, onScrub: (offset: number) => void): Timeline => {
  const group = new THREE.Group()
  const plane = createCanvasPlane(WIDTH, HEIGHT, PX)
  PLOT.h = plane.height - PLOT.y - 44
  group.add(plane.mesh)

  const slider = createSlider(interaction, {
    label: 'scrub',
    min: -span,
    max: 0,
    value: 0,
    length: WIDTH,
    onChange: onScrub,
    format: (v) => (v > -0.05 ? 'live' : `${v.toFixed(1)} s`),
  })
  slider.group.position.set(0, -HEIGHT / 2 - 0.035, 0)
  group.add(slider.group)

  const { ctx, width, height } = plane
  const xAt = (t: number, now: number) => PLOT.x + ((t - (now - span)) / span) * PLOT.w
  let tick = 0

  const draw = ({ now, viewT, pinnedT, live }: TimeView) => {
    ctx.clearRect(0, 0, width, height)
    panel(ctx, { x: 0, y: 0, w: width, h: height }, 22)
    text(ctx, 'seeing across time', 24, 34, 30, COLORS.text, 'left', '600')
    text(ctx, live ? 'live' : `paused · ${(viewT - now).toFixed(2)} s`, width - 24, 34, 28, live ? COLORS.pin : COLORS.command, 'right')
    const legend: [string, string][] = [['lean', COLORS.tilt], ['estimate', COLORS.estimate], ['command', COLORS.command], ['push', COLORS.push]]
    legend.forEach(([label, color], i) => text(ctx, `— ${label}`, 360 + i * 170, 34, 26, color))

    const frames = history.range(now - span, now, 520)
    // Fallen spans and pushes behind the traces.
    frames.forEach((f, i) => {
      if (f.state.fallen) {
        ctx.fillStyle = COLORS.fallen
        ctx.fillRect(xAt(f.t, now), PLOT.y, PLOT.w / frames.length + 1, PLOT.h)
      }
      if (f.push || (i && frames[i - 1].push)) return
    })
    const pushes = history.range(now - span, now, 2400).filter((f) => f.push !== 0)
    ctx.strokeStyle = COLORS.push
    ctx.lineWidth = 3
    for (const f of pushes) {
      ctx.beginPath()
      ctx.moveTo(xAt(f.t, now), PLOT.y)
      ctx.lineTo(xAt(f.t, now), PLOT.y + PLOT.h)
      ctx.stroke()
    }
    traces(ctx, PLOT, [
      { values: frames.map((f) => f.state.theta), color: COLORS.tilt, width: 3 },
      { values: frames.map((f) => f.internals.estimate), color: COLORS.estimate, width: 2 },
    ], LEAN_RANGE)
    traces(ctx, PLOT, [{ values: frames.map((f) => f.internals.command), color: COLORS.command, width: 2 }], [-1.1, 1.1], false)

    // Time axis
    for (let s = 0; s <= span; s += 2) {
      const x = xAt(now - s, now)
      text(ctx, s === 0 ? 'now' : `-${s} s`, x, height - 20, 22, COLORS.dim, 'center')
    }
    const marker = (t: number, color: string, label: string) => {
      const x = xAt(t, now)
      ctx.strokeStyle = color
      ctx.lineWidth = 4
      ctx.beginPath()
      ctx.moveTo(x, PLOT.y - 8)
      ctx.lineTo(x, PLOT.y + PLOT.h + 8)
      ctx.stroke()
      text(ctx, label, x, PLOT.y - 4, 20, color, 'center')
    }
    if (pinnedT !== null && pinnedT >= now - span) marker(pinnedT, COLORS.pin, 'pinned')
    if (!live) marker(viewT, '#ffffff', '')
    plane.commit()
  }

  return {
    group,
    update: (view) => {
      if (tick++ % 2 === 0) draw(view)
    },
    setOffset: (offset) => slider.set(offset),
  }
}
