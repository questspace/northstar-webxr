/**
 * 1€ Filter — Speed-adaptive low-pass filter for noisy signals.
 * Reference: Casiez et al. (2012) "1€ filter: a simple speed-based
 * low-pass filter for noisy input in interactive systems"
 *
 * - Low speed → aggressive smoothing (removes jitter)
 * - High speed → minimal smoothing (preserves responsiveness)
 */
import type { HeadPose, Quat, Vec3 } from '../stores/tracking'

const smoothingFactor = (dt: number, cutoff: number) => {
  const r = 2 * Math.PI * cutoff * dt
  return r / (r + 1)
}

const lowPass = (prev: number, raw: number, alpha: number) =>
  prev + alpha * (raw - prev)

export const createOneEuroFilter = (
  minCutoff = 1.0,
  beta = 0.007,
  dCutoff = 1.0,
) => {
  let prevRaw = 0
  let prevFiltered = 0
  let prevDx = 0
  let prevTime = -1

  return (value: number, timestamp: number): number => {
    if (prevTime < 0) {
      prevRaw = value
      prevFiltered = value
      prevTime = timestamp
      return value
    }

    const dt = Math.max(timestamp - prevTime, 1e-6)
    prevTime = timestamp

    // Estimate derivative
    const dx = (value - prevRaw) / dt
    prevRaw = value

    // Filter derivative to reduce noise
    const alphaDx = smoothingFactor(dt, dCutoff)
    const filteredDx = lowPass(prevDx, dx, alphaDx)
    prevDx = filteredDx

    // Adaptive cutoff based on speed
    const cutoff = minCutoff + beta * Math.abs(filteredDx)
    const alpha = smoothingFactor(dt, cutoff)
    const filtered = lowPass(prevFiltered, value, alpha)
    prevFiltered = filtered

    return filtered
  }
}

/** Component-wise 1€ filter for a fixed-length vector */
export const createVectorFilter = (
  size: number,
  minCutoff = 1.0,
  beta = 0.007,
  dCutoff = 1.0,
) => {
  const filters = Array.from({ length: size }, () => createOneEuroFilter(minCutoff, beta, dCutoff))
  return (values: number[], timestamp: number) => values.map((v, i) => filters[i](v, timestamp))
}

/**
 * 6DOF pose filter. Position and quaternion are filtered component-wise; the
 * quaternion is kept on one hemisphere (q and -q are the same rotation) and
 * renormalised, so there is no ±180° wrap as with Euler angles.
 */
export const createPoseFilter = (
  minCutoff = 1.0,
  beta = 0.007,
  dCutoff = 1.0,
) => {
  const filterPosition = createVectorFilter(3, minCutoff, beta, dCutoff)
  const filterQuaternion = createVectorFilter(4, minCutoff, beta, dCutoff)
  let prevQ: Quat = [0, 0, 0, 1]

  return (pose: HeadPose, timestamp: number): HeadPose => {
    const dot = pose.quaternion.reduce((sum, v, i) => sum + v * prevQ[i], 0)
    const aligned = dot < 0 ? (pose.quaternion.map((v) => -v) as Quat) : pose.quaternion
    const q = filterQuaternion(aligned, timestamp)
    const norm = Math.hypot(...q) || 1
    prevQ = q.map((v) => v / norm) as Quat
    return {
      ...pose,
      position: filterPosition(pose.position, timestamp) as Vec3,
      quaternion: prevQ,
    }
  }
}
