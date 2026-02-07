import { useEffect, useRef } from 'react'
import { useTrackingStore } from '../stores/tracking'
import { createPoseFilter } from '../lib/one-euro-filter'

const XR50_WS_URL = 'ws://localhost:8080'
const RECONNECT_DELAY = 2000

/**
 * Connects to the XR50 WebSocket bridge, applies 1€ filtering,
 * and pushes filtered poses into the Zustand store.
 */
export const useXR50 = () => {
  const wsRef = useRef<WebSocket | null>(null)
  const filterRef = useRef(createPoseFilter(1.0, 0.007, 1.0))
  const frameCountRef = useRef(0)
  const lastFpsTimeRef = useRef(Date.now())

  const { setXR50Connected, setHeadPose, setXR50Fps } = useTrackingStore.getState()

  useEffect(() => {
    let reconnectTimer: ReturnType<typeof setTimeout>
    let alive = true

    const connect = () => {
      if (!alive) return
      const ws = new WebSocket(XR50_WS_URL)
      wsRef.current = ws

      ws.onopen = () => {
        console.log('[XR50] connected')
        useTrackingStore.getState().setXR50Connected(true)
      }

      ws.onmessage = (e) => {
        try {
          const raw = JSON.parse(e.data) as {
            x: number; y: number; z: number
            roll: number; pitch: number; yaw: number
            t?: number
          }

          const now = performance.now() / 1000
          const filtered = filterRef.current(raw, raw.t ?? now)

          useTrackingStore.getState().setHeadPose({
            ...filtered,
            timestamp: raw.t ?? now,
          })

          // FPS counter
          frameCountRef.current++
          const nowMs = Date.now()
          if (nowMs - lastFpsTimeRef.current > 1000) {
            useTrackingStore.getState().setXR50Fps(frameCountRef.current)
            frameCountRef.current = 0
            lastFpsTimeRef.current = nowMs
          }
        } catch { /* skip malformed */ }
      }

      ws.onclose = () => {
        console.log('[XR50] disconnected')
        useTrackingStore.getState().setXR50Connected(false)
        if (alive) reconnectTimer = setTimeout(connect, RECONNECT_DELAY)
      }

      ws.onerror = () => ws.close()
    }

    connect()

    return () => {
      alive = false
      clearTimeout(reconnectTimer)
      wsRef.current?.close()
    }
  }, [])
}
