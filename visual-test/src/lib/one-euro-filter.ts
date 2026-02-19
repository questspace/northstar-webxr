/**
 * 1€ Filter — Speed-adaptive low-pass filter for noisy signals.
 * Reference: Casiez et al. (2012) "1€ filter: a simple speed-based
 * low-pass filter for noisy input in interactive systems"
 *
 * - Low speed → aggressive smoothing (removes jitter)
 * - High speed → minimal smoothing (preserves responsiveness)
 */

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

/** Bundle of 6 independent 1€ filters for a full 6DOF pose */
export const createPoseFilter = (
  minCutoff = 1.0,
  beta = 0.007,
  dCutoff = 1.0,
) => {
  const fx = createOneEuroFilter(minCutoff, beta, dCutoff)
  const fy = createOneEuroFilter(minCutoff, beta, dCutoff)
  const fz = createOneEuroFilter(minCutoff, beta, dCutoff)
  const fRoll = createOneEuroFilter(minCutoff, beta, dCutoff)
  const fPitch = createOneEuroFilter(minCutoff, beta, dCutoff)
  const fYaw = createOneEuroFilter(minCutoff, beta, dCutoff)

  return (
    pose: { x: number; y: number; z: number; roll: number; pitch: number; yaw: number },
    timestamp: number,
  ) => ({
    x: fx(pose.x, timestamp),
    y: fy(pose.y, timestamp),
    z: fz(pose.z, timestamp),
    roll: fRoll(pose.roll, timestamp),
    pitch: fPitch(pose.pitch, timestamp),
    yaw: fYaw(pose.yaw, timestamp),
  })
}
