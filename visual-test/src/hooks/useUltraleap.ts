import { useEffect, useRef } from 'react'
import { useTrackingStore, type HandData, type HandJoint } from '../stores/tracking'

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
  mcpPosition: number[]
  pipPosition: number[]
  dipPosition: number[]
  tipPosition: number[]
}

interface LeapHand {
  id: number
  type: 'left' | 'right'
  palmPosition: number[]
}

interface LeapFrame {
  hands?: LeapHand[]
  pointables?: LeapPointable[]
}

const parseHand = (hand: LeapHand, pointables: LeapPointable[]): HandData => {
  const fingerPointables = pointables
    .filter((p) => p.handId === hand.id)
    .sort((a, b) => a.type - b.type)

  const fingers: HandJoint[][] = fingerPointables.map((f) =>
    [f.mcpPosition, f.pipPosition, f.dipPosition, f.tipPosition]
      .filter(Boolean)
      .map((pos) => ({ position: pos as [number, number, number] })),
  )

  return {
    id: hand.id,
    type: hand.type,
    palmPosition: hand.palmPosition as [number, number, number],
    fingers,
  }
}

/**
 * Connects to the Ultraleap Gemini WebSocket and pushes
 * hand data into the Zustand store.
 */
export const useUltraleap = (enabled = true) => {
  const wsRef = useRef<WebSocket | null>(null)
  const frameCountRef = useRef(0)
  const lastFpsTimeRef = useRef(Date.now())

  useEffect(() => {
    if (!enabled) return

    let reconnectTimer: ReturnType<typeof setTimeout>
    let alive = true

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
      const store = useTrackingStore.getState()
      store.setLeapStatus('Connecting to Ultraleap websocket on localhost:6437')

      const ws = new WebSocket(LEAP_WS_URL)
      wsRef.current = ws

      ws.onopen = () => {
        const next = useTrackingStore.getState()
        next.setLeapConnected(true)
        next.setLeapStatus('Ultraleap connected')
        ws.send(JSON.stringify({ optimizeHMD: true }))
        ws.send(JSON.stringify({ background: true }))
        ws.send(JSON.stringify({ focused: true }))
        ws.send(JSON.stringify({ enableGestures: true }))
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

          // FPS counter
          frameCountRef.current++
          const now = Date.now()
          if (now - lastFpsTimeRef.current > 1000) {
            useTrackingStore.getState().setLeapFps(frameCountRef.current)
            frameCountRef.current = 0
            lastFpsTimeRef.current = now
          }
        } catch { /* skip non-JSON */ }
      }

      ws.onclose = () => {
        void retry()
      }

      ws.onerror = () => ws.close()
    }

    connect()

    return () => {
      alive = false
      clearTimeout(reconnectTimer)
      wsRef.current?.close()
      useTrackingStore.getState().setLeapStatus('Ultraleap disabled')
    }
  }, [enabled])
}
