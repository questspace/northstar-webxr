import { useEffect } from 'react'
import * as THREE from 'three'
import { useTrackingStore, type HandData, type Quat, type Vec3 } from '../stores/tracking'

/** Synthetic open hand in the Leap frame (mm): fingers along +Y, palm facing the sensor. */
const hand = (type: 'left' | 'right', x: number): HandData => {
  const fingers = Array.from({ length: 5 }, (_, finger) =>
    Array.from({ length: 5 }, (_, joint): Vec3 => {
      const spread = (finger - 2) * 22
      const curl = Math.sin(joint * 0.8 + finger * 0.4) * 8
      return [x + spread, 100 + joint * 32, 110 - joint * 12 + curl]
    }),
  )

  return {
    id: type === 'left' ? 1 : 2,
    type,
    palmPosition: [x, 120, 100],
    palmNormal: [0, 0, 1],
    direction: [0, 1, 0],
    wrist: [x, 60, 120],
    pinchStrength: 0,
    fingers,
    fingerWidth: Array(5).fill(18),
  }
}

export const useHeadsetSimulator = (enabled: boolean) => {
  useEffect(() => {
    if (!enabled) return

    let frame = 0
    let raf = 0
    const store = useTrackingStore.getState()

    store.setXR50Connected(true)
    store.setLeapConnected(true)
    store.setXR50Status('XR50 simulated')
    store.setLeapStatus('Ultraleap simulated')
    store.setXR50Fps(90)
    store.setLeapFps(90)

    const tick = () => {
      const t = performance.now() / 1000
      const sway = new THREE.Euler(
        THREE.MathUtils.degToRad(Math.sin(t * 0.6) * 8),  // pitch
        THREE.MathUtils.degToRad(Math.sin(t * 0.5) * 16), // yaw
        THREE.MathUtils.degToRad(Math.sin(t * 0.8) * 3),  // roll
        'YXZ',
      )
      useTrackingStore.getState().setHeadPose({
        position: [
          Math.sin(t * 0.55) * 0.12,
          Math.sin(t * 0.35) * 0.04,
          Math.cos(t * 0.45) * 0.12,
        ],
        quaternion: new THREE.Quaternion().setFromEuler(sway).toArray() as Quat,
        confidence: 1,
        timestamp: t,
        receivedAt: Date.now(),
      })

      const wave = Math.sin(t * 1.4) * 20
      useTrackingStore.getState().setHands(
        hand('left', -85 - wave),
        hand('right', 85 + wave),
      )

      frame += 1
      if (frame % 30 === 0) {
        useTrackingStore.getState().setXR50Fps(90)
        useTrackingStore.getState().setLeapFps(90)
      }

      raf = requestAnimationFrame(tick)
    }

    raf = requestAnimationFrame(tick)

    return () => {
      cancelAnimationFrame(raf)
      const next = useTrackingStore.getState()
      next.setXR50Connected(false)
      next.setLeapConnected(false)
      next.setXR50Status('XR50 simulator stopped')
      next.setLeapStatus('Ultraleap simulator stopped')
      next.setHands(null, null)
    }
  }, [enabled])
}
