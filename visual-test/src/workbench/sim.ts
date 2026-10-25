/**
 * The thing on the bench: a two-wheeled self-balancing robot, simulated as a
 * cart-pole driven by wheel torque (the motor's reaction torque acts on the
 * body). Noisy IMU + encoders → complementary-filter tilt estimate → PID with a
 * slow position hold → motor command. Every internal variable is in the frame
 * record, which is what the instruments show. Pure functions and a tiny closure
 * for the controller memory; deterministic for a given seed, so parameter
 * sweeps replay the same disturbance.
 */

export interface Params {
  kp: number
  ki: number
  kd: number
  /** Complementary filter weight on the gyro integral */
  alpha: number
  /** Sensor noise and gyro bias on/off */
  noise: boolean
  /** Position hold: lean setpoint per metre of drift and per m/s */
  kx: number
  kv: number
}

export interface State {
  /** Along the track (m), forward speed (m/s) */
  x: number
  v: number
  /** Lean from upright (rad, + = forward), lean rate (rad/s) */
  theta: number
  omega: number
  /** Body acceleration last step (m/s²), felt by the accelerometer */
  a: number
  fallen: boolean
}

export interface Sensors {
  gyro: number
  accelTilt: number
  wheelSpeed: number
  battery: number
  current: number
}

export interface Internals {
  estimate: number
  setpoint: number
  error: number
  p: number
  i: number
  d: number
  command: number
  torque: number
  saturated: boolean
}

export interface Frame {
  t: number
  state: State
  sensors: Sensors
  internals: Internals
  /** Impulse applied this step (N·s), 0 when none */
  push: number
}

export const DT = 1 / 200
export const FALL_ANGLE = 1.0
export const TRACK = 0.3
export const PUSH_IMPULSE = 0.06

export const DEFAULT_PARAMS: Params = { kp: 9, ki: 3, kd: 0.5, alpha: 0.98, noise: true, kx: 0.25, kv: 0.35 }

// Plant
const G = 9.81
const M_WHEELS = 0.25
const M_BODY = 0.5
const L = 0.11
const I_BODY = (M_BODY * L * L) / 3
const FRICTION = 0.15
export const WHEEL_RADIUS = 0.035
const TORQUE_MAX = 0.35
const PUSH_HEIGHT = 2 * L

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))

/** mulberry32: small, seedable, good enough for sensor noise */
const rng = (seed: number) => () => {
  seed = (seed + 0x6d2b79f5) | 0
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed)
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296
}
const gaussian = (next: () => number) => (next() + next() + next() - 1.5) * 1.414

export const initialState = (): State => ({ x: 0, v: 0, theta: 0.02, omega: 0, a: 0, fallen: false })

const sense = (s: State, params: Params, noise: () => number, bias: number): Sensors => {
  const n = (sd: number) => (params.noise ? gaussian(noise) * sd : 0)
  const sin = Math.sin(s.theta)
  const cos = Math.cos(s.theta)
  return {
    gyro: s.omega + (params.noise ? bias : 0) + n(0.02),
    // Specific force in the body frame: gravity plus the cart's own acceleration.
    accelTilt: Math.atan2(G * sin - s.a * cos, G * cos + s.a * sin) + n(0.03),
    wheelSpeed: Math.round((s.v / WHEEL_RADIUS) / 0.05) * 0.05,
    battery: 7.4 + n(0.01),
    current: 0,
  }
}

const solve2 = (a11: number, a12: number, a21: number, a22: number, b1: number, b2: number) => {
  const det = a11 * a22 - a12 * a21
  return [(b1 * a22 - a12 * b2) / det, (a11 * b2 - a21 * b1) / det]
}

/** One physics step: wheel torque, friction, a push impulse at the top of the body. */
const integrate = (s: State, torque: number, push: number): State => {
  if (s.fallen) return { ...s, v: s.v * 0.9, omega: 0, a: 0 }
  const sin = Math.sin(s.theta)
  const cos = Math.cos(s.theta)
  const [a, alpha] = solve2(
    M_WHEELS + M_BODY, M_BODY * L * cos,
    M_BODY * L * cos, I_BODY + M_BODY * L * L,
    torque / WHEEL_RADIUS + M_BODY * L * s.omega * s.omega * sin - FRICTION * s.v,
    M_BODY * G * L * sin - torque,
  )
  let v = s.v + a * DT + push / (M_WHEELS + M_BODY)
  const omega = s.omega + alpha * DT + (push * PUSH_HEIGHT) / (I_BODY + M_BODY * L * L)
  let x = s.x + v * DT
  const theta = s.theta + omega * DT
  if (Math.abs(x) > TRACK) {
    x = clamp(x, -TRACK, TRACK)
    v = -0.3 * v
  }
  const fallen = Math.abs(theta) > FALL_ANGLE
  return { x, v, theta: fallen ? Math.sign(theta) * 1.35 : theta, omega: fallen ? 0 : omega, a, fallen }
}

export interface Sim {
  params: Params
  readonly frame: Frame
  /** Advances one step (DT); `push` is an impulse in N·s applied this step. */
  step: (push?: number) => Frame
  reset: () => void
}

export const createSim = (params: Params, seed = 1): Sim => {
  let noise = rng(seed)
  let bias = 0.03
  let integral = 0
  let estimate = 0
  let frame: Frame = {
    t: 0,
    state: initialState(),
    sensors: { gyro: 0, accelTilt: 0, wheelSpeed: 0, battery: 7.4, current: 0 },
    internals: { estimate: 0, setpoint: 0, error: 0, p: 0, i: 0, d: 0, command: 0, torque: 0, saturated: false },
    push: 0,
  }

  const sim: Sim = {
    params,
    get frame() { return frame },
    step: (push = 0) => {
      const { params: p } = sim
      const s = frame.state
      const sensors = sense(s, p, noise, bias)
      estimate = p.alpha * (estimate + sensors.gyro * DT) + (1 - p.alpha) * sensors.accelTilt
      // Position hold: to come back you first have to lean the other way.
      const setpoint = clamp(-(p.kx * s.x + p.kv * s.v), -0.1, 0.1)
      const error = estimate - setpoint
      integral = s.fallen ? 0 : clamp(integral + error * DT, -0.5, 0.5)
      const terms = { p: p.kp * error, i: p.ki * integral, d: p.kd * sensors.gyro }
      const raw = terms.p + terms.i + terms.d
      const command = s.fallen ? 0 : clamp(raw, -1, 1)
      const torque = command * TORQUE_MAX
      sensors.current = Math.abs(command) * 2.5
      sensors.battery -= Math.abs(command) * 0.6
      frame = {
        t: frame.t + DT,
        state: integrate(s, torque, push),
        sensors,
        internals: { estimate, setpoint, error, ...terms, command, torque, saturated: Math.abs(raw) > 1 },
        push,
      }
      return frame
    },
    reset: () => {
      noise = rng(seed)
      bias = 0.03
      integral = 0
      estimate = 0
      frame = { ...frame, t: frame.t, state: initialState(), push: 0 }
    },
  }
  return sim
}
