import { useEffect } from 'react'
import { useTrackingStore, type HandData } from '../stores/tracking'

const hand = (type: 'left' | 'right', x: number): HandData => {
  const fingers = Array.from({ length: 5 }, (_, finger) =>
    Array.from({ length: 4 }, (_, joint) => {
      const spread = (finger - 2) * 22
      const curl = Math.sin(joint * 0.8 + finger * 0.4) * 8
      return {
        position: [
          x + spread,
          160 + joint * 36,
          -45 - joint * 18 + curl,
        ] as [number, number, number],
      }
    }),
  )

  return {
    id: type === 'left' ? 1 : 2,
    type,
    palmPosition: [x, 120, -30],
    fingers,
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
      useTrackingStore.getState().setHeadPose({
        x: Math.sin(t * 0.55) * 0.12,
        y: Math.sin(t * 0.35) * 0.04,
        z: Math.cos(t * 0.45) * 0.12,
        roll: Math.sin(t * 0.8) * 3,
        pitch: Math.sin(t * 0.6) * 8,
        yaw: Math.sin(t * 0.5) * 16,
        timestamp: t,
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
