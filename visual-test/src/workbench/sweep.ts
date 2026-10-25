/**
 * Seeing across possibilities: the same disturbance replayed under many
 * controller settings, side by side. Each cell is a short deterministic
 * simulation from rest with a push at 0.1 s.
 */
import { createSim, DT, type Params } from './sim'

export interface Response {
  kp: number
  kd: number
  /** Lean trace, sampled every 4 steps (50 Hz) */
  theta: number[]
  fell: boolean
  /** Seconds until the lean and its rate stayed small (the full window when they never did) */
  settle: number
}

export const SWEEP_SECONDS = 3
export const KP_GRID = [0.3, 2, 5, 9, 16]
export const KD_GRID = [0.05, 0.2, 0.4, 0.7, 1.2]

export const respond = (params: Params, push: number, seconds = SWEEP_SECONDS): Response => {
  const sim = createSim(params, 7)
  const steps = Math.round(seconds / DT)
  const theta: number[] = []
  let settle = 0
  for (let n = 0; n < steps; n++) {
    const f = sim.step(n === Math.round(0.1 / DT) ? push : 0)
    if (n % 4 === 0) theta.push(f.state.theta)
    if (Math.abs(f.state.theta) > 0.03 || Math.abs(f.state.omega) > 0.3) settle = f.t
  }
  return { kp: params.kp, kd: params.kd, theta, fell: sim.frame.state.fallen, settle }
}

/** Rows: kd (bottom to top), columns: kp (left to right). */
export const sweep = (base: Params, push: number) =>
  KD_GRID.map((kd) => KP_GRID.map((kp) => respond({ ...base, kp, kd }, push)))
