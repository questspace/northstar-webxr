/** Canvas-textured planes for the instruments, plus the few drawing helpers they share. */
import * as THREE from 'three'

export interface CanvasPlane {
  mesh: THREE.Mesh
  ctx: CanvasRenderingContext2D
  width: number
  height: number
  /** Uploads the canvas after drawing. */
  commit: () => void
}

export const createCanvasPlane = (widthM: number, heightM: number, px: number): CanvasPlane => {
  const canvas = document.createElement('canvas')
  canvas.width = px
  canvas.height = Math.round((px * heightM) / widthM)
  const ctx = canvas.getContext('2d')!
  const texture = new THREE.CanvasTexture(canvas)
  texture.colorSpace = THREE.SRGBColorSpace
  texture.minFilter = THREE.LinearFilter
  texture.anisotropy = 4
  const mesh = new THREE.Mesh(
    new THREE.PlaneGeometry(widthM, heightM),
    new THREE.MeshBasicMaterial({ map: texture, transparent: true, side: THREE.DoubleSide }),
  )
  return { mesh, ctx, width: canvas.width, height: canvas.height, commit: () => { texture.needsUpdate = true } }
}

export interface Rect { x: number; y: number; w: number; h: number }
export interface Trace { values: number[]; color: string; width?: number }

export const COLORS = {
  panel: 'rgba(10, 16, 26, 0.82)',
  grid: 'rgba(160, 190, 230, 0.18)',
  text: '#dbe7ff',
  dim: '#8aa0c0',
  tilt: '#ffffff',
  estimate: '#63d2ff',
  command: '#ffb347',
  push: '#ff5c5c',
  pin: '#7ef0a0',
  fallen: 'rgba(255, 70, 70, 0.18)',
}

export const panel = (ctx: CanvasRenderingContext2D, r: Rect, radius = 18) => {
  ctx.fillStyle = COLORS.panel
  ctx.beginPath()
  ctx.roundRect(r.x, r.y, r.w, r.h, radius)
  ctx.fill()
}

export const text = (ctx: CanvasRenderingContext2D, s: string, x: number, y: number, size: number, color = COLORS.text, align: CanvasTextAlign = 'left', weight = '500') => {
  ctx.font = `${weight} ${size}px system-ui, sans-serif`
  ctx.fillStyle = color
  ctx.textAlign = align
  ctx.textBaseline = 'middle'
  ctx.fillText(s, x, y)
}

/** Traces over `r`, each mapped from [min, max] to the rect's height. */
export const traces = (ctx: CanvasRenderingContext2D, r: Rect, list: Trace[], [min, max]: [number, number], zeroLine = true) => {
  const y = (v: number) => r.y + r.h - ((Math.min(max, Math.max(min, v)) - min) / (max - min)) * r.h
  if (zeroLine && min < 0 && max > 0) {
    ctx.strokeStyle = COLORS.grid
    ctx.lineWidth = 2
    ctx.beginPath()
    ctx.moveTo(r.x, y(0))
    ctx.lineTo(r.x + r.w, y(0))
    ctx.stroke()
  }
  for (const trace of list) {
    const n = trace.values.length
    if (n < 2) continue
    ctx.strokeStyle = trace.color
    ctx.lineWidth = trace.width ?? 3
    ctx.lineJoin = 'round'
    ctx.beginPath()
    trace.values.forEach((v, i) => {
      const x = r.x + (i / (n - 1)) * r.w
      i ? ctx.lineTo(x, y(v)) : ctx.moveTo(x, y(v))
    })
    ctx.stroke()
  }
}

/** A signed horizontal bar from the rect's centre, for live magnitudes. */
export const bar = (ctx: CanvasRenderingContext2D, r: Rect, value: number, [min, max]: [number, number], color: string) => {
  ctx.fillStyle = 'rgba(255,255,255,0.08)'
  ctx.fillRect(r.x, r.y, r.w, r.h)
  const zero = r.x + ((0 - min) / (max - min)) * r.w
  const at = r.x + ((Math.min(max, Math.max(min, value)) - min) / (max - min)) * r.w
  ctx.fillStyle = color
  ctx.fillRect(Math.min(zero, at), r.y, Math.abs(at - zero), r.h)
}
