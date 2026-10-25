import { useEffect } from 'react'
import { useTrackingStore, type Quat, type Vec3 } from '../stores/tracking'
import { createPoseFilter } from '../lib/one-euro-filter'
import { xr50ToThree } from '../lib/transforms'

const XR50_WS_URL = 'ws://localhost:8080'
const XR50_HTTP_URL = 'http://localhost:8080'
const RECONNECT_DELAY = 2000
// A socket can stay open yet stop delivering poses (SLAM stall upstream).
// If no message arrives within this window, force a reconnect.
const STALL_TIMEOUT = 2500

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
 * Connects to the XR50 WebSocket bridge, applies 1€ filtering, and pushes
 * filtered poses into the Zustand store until the returned function is called.
 */
export const connectXR50 = () => {
  // Light smoothing only: SLAM output is already filtered, and a 1 Hz cutoff
  // made the view trail head motion by a visible fraction of a second.
  const filter = createPoseFilter(8, 2.0, 1.0)
  let lastPosition: Vec3 = [0, 0, 0]
  let lastQuaternion: Quat = [0, 0, 0, 1]
  let frameCount = 0
  let lastFpsTime = Date.now()
  let ws: WebSocket | null = null
  let reconnectTimer: ReturnType<typeof setTimeout>
  let stallTimer: ReturnType<typeof setTimeout>
  let alive = true
  let stalled = false

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
    const socket = new WebSocket(XR50_WS_URL)
    ws = socket

    // Reset the stall watchdog on every frame; if it ever fires, the stream
    // has gone silent, so drop the socket to trigger a fresh reconnect.
    const armWatchdog = () => {
      clearTimeout(stallTimer)
      stallTimer = setTimeout(() => {
        stalled = true
        const s = useTrackingStore.getState()
        s.setXR50Stalled(true)
        s.setXR50Status('XR50 stream stalled - reconnecting')
        socket.close()
      }, STALL_TIMEOUT)
    }

    socket.onopen = () => {
      const next = useTrackingStore.getState()
      next.setXR50Connected(true)
      next.setXR50Stalled(false)
      stalled = false
      next.setXR50Status('XR50 connected')
      armWatchdog()
    }

    socket.onmessage = (e) => {
      armWatchdog()
      if (stalled) {
        stalled = false
        useTrackingStore.getState().setXR50Stalled(false)
      }
      try {
        const raw = JSON.parse(e.data) as {
          x: number; y: number; z: number
          q: Quat
          c?: number
          t?: number
          tw?: number
        }

        const ts = toSeconds(raw.t)
        const pose = xr50ToThree([raw.x, raw.y, raw.z], raw.q)
        // While SLAM is lost the translation saturates (±131072 m), and a
        // device that is not tracking at all sends a non-unit quaternion
        // ([0,0,0,0.5]): keep the last good values instead.
        if (pose.position.every((v) => Math.abs(v) < 100)) lastPosition = pose.position
        if (Math.abs(Math.hypot(...raw.q) - 1) < 0.1) lastQuaternion = pose.quaternion

        useTrackingStore.getState().setHeadPose(filter({
          position: lastPosition,
          quaternion: lastQuaternion,
          confidence: raw.c ?? 1,
          timestamp: ts,
          receivedAt: raw.tw ?? Date.now(),
        }, ts))

        frameCount++
        const nowMs = Date.now()
        if (nowMs - lastFpsTime > 1000) {
          useTrackingStore.getState().setXR50Fps(frameCount)
          frameCount = 0
          lastFpsTime = nowMs
        }
      } catch { /* skip malformed */ }
    }

    socket.onclose = () => {
      clearTimeout(stallTimer)
      const next = useTrackingStore.getState()
      next.setXR50Connected(false)
      if (!stalled) next.setXR50Status('XR50 disconnected')
      if (alive) reconnectTimer = setTimeout(connect, RECONNECT_DELAY)
    }

    socket.onerror = () => socket.close()
  }

  void connect()

  return () => {
    alive = false
    clearTimeout(reconnectTimer)
    clearTimeout(stallTimer)
    ws?.close()
    const store = useTrackingStore.getState()
    store.setXR50Stalled(false)
    store.setXR50Status('XR50 disabled')
  }
}

export const useXR50 = (enabled = true) => {
  useEffect(() => (enabled ? connectXR50() : undefined), [enabled])
}
