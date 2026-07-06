import { useEffect, useRef } from 'react'
import { useTrackingStore } from '../stores/tracking'
import { createPoseFilter } from '../lib/one-euro-filter'

const XR50_WS_URL = 'ws://localhost:8080'
const XR50_HTTP_URL = 'http://localhost:8080'
const RECONNECT_DELAY = 2000

const canReachBridge = async () => {
  const controller = new AbortController()
  const timeout = window.setTimeout(() => controller.abort(), 800)
  try {
    await fetch(XR50_HTTP_URL, {
      mode: 'no-cors',
      cache: 'no-store',
      signal: controller.signal,
    })
    return true
  } catch {
    return false
  } finally {
    window.clearTimeout(timeout)
  }
}

const toSeconds = (t: number | undefined): number => {
  if (typeof t !== 'number' || !Number.isFinite(t)) return performance.now() / 1000
  // XR50 wire timestamp is microseconds; convert when needed.
  if (t > 1_000_000_000) return t / 1_000_000
  // Accept millisecond-based timestamps from other bridges.
  if (t > 1_000_000) return t / 1_000
  return t
}

/**
 * Connects to the XR50 WebSocket bridge, applies 1€ filtering,
 * and pushes filtered poses into the Zustand store.
 */
export const useXR50 = (enabled = true) => {
  const wsRef = useRef<WebSocket | null>(null)
  const filterRef = useRef(createPoseFilter(1.0, 0.007, 1.0))
  const frameCountRef = useRef(0)
  const lastFpsTimeRef = useRef(Date.now())

  const { setXR50Connected, setHeadPose, setXR50Fps } = useTrackingStore.getState()

  useEffect(() => {
    if (!enabled) return

    let reconnectTimer: ReturnType<typeof setTimeout>
    let alive = true

    const connect = async () => {
      if (!alive) return
      const store = useTrackingStore.getState()
      store.setXR50Status('Checking XR50 bridge on localhost:8080')

      if (!(await canReachBridge())) {
        store.setXR50Connected(false)
        store.setXR50Status('XR50 bridge not running on localhost:8080')
        reconnectTimer = setTimeout(connect, RECONNECT_DELAY)
        return
      }

      store.setXR50Status('Connecting to XR50 bridge')
      const ws = new WebSocket(XR50_WS_URL)
      wsRef.current = ws

      ws.onopen = () => {
        const next = useTrackingStore.getState()
        next.setXR50Connected(true)
        next.setXR50Status('XR50 connected')
      }

      ws.onmessage = (e) => {
        try {
          const raw = JSON.parse(e.data) as {
            x: number; y: number; z: number
            roll: number; pitch: number; yaw: number
            t?: number
          }

          const ts = toSeconds(raw.t)
          const filtered = filterRef.current(raw, ts)

          useTrackingStore.getState().setHeadPose({
            ...filtered,
            timestamp: ts,
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
        const next = useTrackingStore.getState()
        next.setXR50Connected(false)
        next.setXR50Status('XR50 disconnected')
        if (alive) reconnectTimer = setTimeout(connect, RECONNECT_DELAY)
      }

      ws.onerror = () => ws.close()
    }

    connect()

    return () => {
      alive = false
      clearTimeout(reconnectTimer)
      wsRef.current?.close()
      useTrackingStore.getState().setXR50Status('XR50 disabled')
    }
  }, [enabled])
}
