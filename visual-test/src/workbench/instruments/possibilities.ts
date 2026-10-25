/**
 * Seeing across possibilities: a Kp × Kd grid of the robot's response to the
 * last push, every cell a replay under those gains. Green settles, red falls;
 * the live gains are framed. Touch a cell to adopt its gains.
 */
import * as THREE from 'three'
import type { Interaction } from '../../xr/interaction'
import type { Params } from '../sim'
import { KD_GRID, KP_GRID, SWEEP_SECONDS, sweep, type Response } from '../sweep'
import { createCanvasPlane, panel, text, traces, COLORS } from '../chart'

const SIZE = 0.38
const PX = 1024
const MARGIN = { left: 110, right: 24, top: 90, bottom: 80 }

export interface Possibilities {
  group: THREE.Group
  /** Re-runs the sweep (synchronous, a few ms). */
  compute: (params: Params, push: number) => void
  /** Redraws the frame around the live gains. */
  update: (params: Params) => void
}

const nearest = (grid: number[], v: number) => grid.reduce((best, g, i) => (Math.abs(g - v) < Math.abs(grid[best] - v) ? i : best), 0)

export const createPossibilities = (interaction: Interaction, onPick: (kp: number, kd: number) => void): Possibilities => {
  const group = new THREE.Group()
  const plane = createCanvasPlane(SIZE, SIZE, PX)
  group.add(plane.mesh)
  const { ctx, width, height } = plane
  const cellW = (width - MARGIN.left - MARGIN.right) / KP_GRID.length
  const cellH = (height - MARGIN.top - MARGIN.bottom) / KD_GRID.length
  const cell = (col: number, row: number) => ({
    x: MARGIN.left + col * cellW,
    y: height - MARGIN.bottom - (row + 1) * cellH,
    w: cellW,
    h: cellH,
  })

  let cells: Response[][] = []
  let lastPush = 0
  let highlighted = { col: -1, row: -1 }

  const draw = () => {
    ctx.clearRect(0, 0, width, height)
    panel(ctx, { x: 0, y: 0, w: width, h: height }, 24)
    text(ctx, 'seeing across possibilities', 24, 36, 32, COLORS.text, 'left', '600')
    text(ctx, `response to the last push (${lastPush.toFixed(2)} N·s), ${SWEEP_SECONDS} s each`, 24, 70, 24, COLORS.dim)
    cells.forEach((row, r) =>
      row.forEach((response, c) => {
        const rect = cell(c, r)
        const quality = 1 - Math.min(response.settle / SWEEP_SECONDS, 1)
        ctx.fillStyle = response.fell ? 'rgba(255, 80, 80, 0.32)' : `rgba(110, 230, 150, ${0.08 + quality * 0.4})`
        ctx.fillRect(rect.x + 3, rect.y + 3, rect.w - 6, rect.h - 6)
        traces(ctx, { x: rect.x + 10, y: rect.y + 10, w: rect.w - 20, h: rect.h - 20 }, [{ values: response.theta, color: COLORS.tilt, width: 2 }], [-1.1, 1.1])
        text(ctx, response.fell ? 'fell' : `${response.settle.toFixed(1)} s`, rect.x + rect.w - 8, rect.y + rect.h - 16, 20, COLORS.dim, 'right')
      }),
    )
    if (highlighted.col >= 0) {
      const rect = cell(highlighted.col, highlighted.row)
      ctx.strokeStyle = '#ffffff'
      ctx.lineWidth = 5
      ctx.strokeRect(rect.x + 2, rect.y + 2, rect.w - 4, rect.h - 4)
    }
    KP_GRID.forEach((kp, c) => text(ctx, `${kp}`, MARGIN.left + (c + 0.5) * cellW, height - MARGIN.bottom + 28, 26, COLORS.text, 'center'))
    text(ctx, 'Kp →', width - MARGIN.right, height - 24, 26, COLORS.dim, 'right')
    KD_GRID.forEach((kd, r) => text(ctx, `${kd}`, MARGIN.left - 16, height - MARGIN.bottom - (r + 0.5) * cellH, 26, COLORS.text, 'right'))
    text(ctx, 'Kd ↑', 24, MARGIN.top - 10, 26, COLORS.dim)
    plane.commit()
  }

  interaction.add({
    object: plane.mesh,
    press: {
      size: [SIZE, SIZE],
      travel: 0.01,
      onPress: (local) => {
        const px = ((local.x + SIZE / 2) / SIZE) * width
        const py = ((SIZE / 2 - local.y) / SIZE) * height
        const col = Math.floor((px - MARGIN.left) / cellW)
        const row = Math.floor((height - MARGIN.bottom - py) / cellH)
        if (col >= 0 && col < KP_GRID.length && row >= 0 && row < KD_GRID.length) onPick(KP_GRID[col], KD_GRID[row])
      },
    },
  })

  return {
    group,
    compute: (params, push) => {
      lastPush = push
      cells = sweep(params, push)
      draw()
    },
    update: (params) => {
      const next = { col: nearest(KP_GRID, params.kp), row: nearest(KD_GRID, params.kd) }
      if (next.col === highlighted.col && next.row === highlighted.row) return
      highlighted = next
      draw()
    },
  }
}
