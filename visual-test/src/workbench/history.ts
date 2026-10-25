/** Ring buffer of frames with lookups by time: the "seeing across time" substrate. */
import type { Frame } from './sim'

export interface History {
  push: (frame: Frame) => void
  readonly length: number
  readonly first: Frame | undefined
  readonly last: Frame | undefined
  /** Closest frame at or before `t` (the first frame when `t` is earlier than everything). */
  at: (t: number) => Frame | undefined
  /** Up to `points` frames evenly spread over [from, to]. */
  range: (from: number, to: number, points: number) => Frame[]
  clear: () => void
}

export const createHistory = (capacity: number): History => {
  const frames: Frame[] = []
  let start = 0
  const get = (i: number) => frames[(start + i) % frames.length]
  const length = () => frames.length

  const indexAt = (t: number) => {
    let lo = 0
    let hi = length() - 1
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1
      if (get(mid).t <= t) lo = mid
      else hi = mid - 1
    }
    return lo
  }

  return {
    push: (frame) => {
      if (frames.length < capacity) frames.push(frame)
      else {
        frames[start] = frame
        start = (start + 1) % capacity
      }
    },
    get length() { return length() },
    get first() { return length() ? get(0) : undefined },
    get last() { return length() ? get(length() - 1) : undefined },
    at: (t) => (length() ? get(indexAt(t)) : undefined),
    range: (from, to, points) => {
      if (!length()) return []
      const a = indexAt(from)
      const b = indexAt(to)
      const n = Math.max(1, Math.min(points, b - a + 1))
      return Array.from({ length: n }, (_, k) => get(a + Math.round(((b - a) * k) / Math.max(n - 1, 1))))
    },
    clear: () => {
      frames.length = 0
      start = 0
    },
  }
}
