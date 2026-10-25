import { useEffect } from 'react'
import { useTrackingStore, type HandData, type Vec3 } from '../stores/tracking'

const LEAP_WS_URL = 'ws://localhost:6437/v6.json'
const HYPERION_WS_URL = 'ws://localhost:1024'
const RECONNECT_DELAY = 2000

// The v6 bridge (Ultraleap-Tracking-WS) is WebSocket-only and never answers
// plain HTTP, so reachability must be probed with a WebSocket, not fetch().
const canOpenWebSocket = (url: string) =>
  new Promise<boolean>((resolve) => {
    const ws = new WebSocket(url)
    const timeout = window.setTimeout(() => {
      ws.close()
      resolve(false)
    }, 800)

    ws.onopen = () => {
      window.clearTimeout(timeout)
      ws.close()
      resolve(true)
    }
    ws.onerror = () => {
      window.clearTimeout(timeout)
      resolve(false)
    }
  })

interface LeapPointable {
  handId: number
  type: number // 0=thumb..4=pinky
  carpPosition: Vec3
  mcpPosition: Vec3
  pipPosition: Vec3
  dipPosition: Vec3
  tipPosition: Vec3
  width: number
}

interface LeapHand {
  id: number
  type: 'left' | 'right'
  palmPosition: Vec3
  palmNormal: Vec3
  direction: Vec3
  wrist: Vec3
  pinchStrength: number
}

interface LeapFrame {
  hands?: LeapHand[]
  pointables?: LeapPointable[]
}

const parseHand = (hand: LeapHand, pointables: LeapPointable[]): HandData => {
  const fingers = pointables
    .filter((p) => p.handId === hand.id)
    .sort((a, b) => a.type - b.type)

  return {
    id: hand.id,
    type: hand.type,
    palmPosition: hand.palmPosition,
    palmNormal: hand.palmNormal,
    direction: hand.direction,
    wrist: hand.wrist,
    pinchStrength: hand.pinchStrength,
    fingers: fingers.map((f) => [f.carpPosition, f.mcpPosition, f.pipPosition, f.dipPosition, f.tipPosition]),
    fingerWidth: fingers.map((f) => f.width),
  }
}

/**
 * Connects to the Ultraleap Gemini WebSocket and pushes hand data into the
 * Zustand store until the returned function is called.
 */
export const connectUltraleap = () => {
  let ws: WebSocket | null = null
  let reconnectTimer: ReturnType<typeof setTimeout>
  let alive = true
  let frameCount = 0
  let lastFpsTime = Date.now()

  const retry = async () => {
    const store = useTrackingStore.getState()
    store.setLeapConnected(false)
    store.setHands(null, null)
    const hyperionRunning = await canOpenWebSocket(HYPERION_WS_URL)
    store.setLeapStatus(
      hyperionRunning
        ? 'Ultraleap v6 bridge not running on localhost:6437 (Hyperion detected on 1024; start Ultraleap-Tracking-WS)'
        : 'Ultraleap websocket not running on localhost:6437',
    )
    if (alive) reconnectTimer = setTimeout(connect, RECONNECT_DELAY)
  }

  const connect = () => {
    if (!alive) return
    useTrackingStore.getState().setLeapStatus('Connecting to Ultraleap websocket on localhost:6437')

    ws = new WebSocket(LEAP_WS_URL)

    ws.onopen = () => {
      const next = useTrackingStore.getState()
      next.setLeapConnected(true)
      next.setLeapStatus('Ultraleap connected')
      ws?.send(JSON.stringify({ optimizeHMD: true }))
      ws?.send(JSON.stringify({ background: true }))
      ws?.send(JSON.stringify({ focused: true }))
      ws?.send(JSON.stringify({ enableGestures: true }))
    }

    ws.onmessage = (e) => {
      try {
        const frame = JSON.parse(e.data) as LeapFrame
        if (!frame.hands) return

        const pointables = frame.pointables ?? []
        let left: HandData | null = null
        let right: HandData | null = null

        for (const h of frame.hands) {
          const parsed = parseHand(h, pointables)
          if (h.type === 'left') left = parsed
          else if (h.type === 'right') right = parsed
        }

        useTrackingStore.getState().setHands(left, right)

        frameCount++
        const now = Date.now()
        if (now - lastFpsTime > 1000) {
          useTrackingStore.getState().setLeapFps(frameCount)
          frameCount = 0
          lastFpsTime = now
        }
      } catch { /* skip non-JSON */ }
    }

    ws.onclose = () => {
      void retry()
    }

    ws.onerror = () => ws?.close()
  }

  connect()

  return () => {
    alive = false
    clearTimeout(reconnectTimer)
    ws?.close()
    useTrackingStore.getState().setLeapStatus('Ultraleap disabled')
  }
}

export const useUltraleap = (enabled = true) => {
  useEffect(() => (enabled ? connectUltraleap() : undefined), [enabled])
}
