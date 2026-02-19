import { useEffect, useRef } from 'react'
import { useTrackingStore, type HandData, type HandJoint } from '../stores/tracking'

const LEAP_WS_URL = 'ws://localhost:6437/v6.json'
const RECONNECT_DELAY = 2000

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
export const useUltraleap = () => {
  const wsRef = useRef<WebSocket | null>(null)
  const frameCountRef = useRef(0)
  const lastFpsTimeRef = useRef(Date.now())

  useEffect(() => {
    let reconnectTimer: ReturnType<typeof setTimeout>
    let alive = true

    const connect = () => {
      if (!alive) return
      const ws = new WebSocket(LEAP_WS_URL)
      wsRef.current = ws

      ws.onopen = () => {
        console.log('[Ultraleap] connected')
        useTrackingStore.getState().setLeapConnected(true)
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
        console.log('[Ultraleap] disconnected')
        useTrackingStore.getState().setLeapConnected(false)
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
