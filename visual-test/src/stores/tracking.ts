import { create } from 'zustand'

export interface HeadPose {
  x: number
  y: number
  z: number
  roll: number
  pitch: number
  yaw: number
  timestamp: number
}

export interface HandJoint {
  position: [number, number, number]
}

export interface HandData {
  id: number
  type: 'left' | 'right'
  palmPosition: [number, number, number]
  fingers: HandJoint[][] // 5 fingers × 4 joints each (MCP, PIP, DIP, TIP)
}

interface TrackingState {
  // Connection states
  xr50Connected: boolean
  leapConnected: boolean

  // Head tracking (filtered)
  headPose: HeadPose

  // Hand tracking
  leftHand: HandData | null
  rightHand: HandData | null

  // Telemetry
  xr50Fps: number
  leapFps: number

  // Actions
  setXR50Connected: (connected: boolean) => void
  setLeapConnected: (connected: boolean) => void
  setHeadPose: (pose: HeadPose) => void
  setHands: (left: HandData | null, right: HandData | null) => void
  setXR50Fps: (fps: number) => void
  setLeapFps: (fps: number) => void
}

export const useTrackingStore = create<TrackingState>()((set) => ({
  xr50Connected: false,
  leapConnected: false,
  headPose: { x: 0, y: 0, z: 0, roll: 0, pitch: 0, yaw: 0, timestamp: 0 },
  leftHand: null,
  rightHand: null,
  xr50Fps: 0,
  leapFps: 0,

  setXR50Connected: (connected) => set({ xr50Connected: connected }),
  setLeapConnected: (connected) => set({ leapConnected: connected }),
  setHeadPose: (pose) => set({ headPose: pose }),
  setHands: (left, right) => set({ leftHand: left, rightHand: right }),
  setXR50Fps: (fps) => set({ xr50Fps: fps }),
  setLeapFps: (fps) => set({ leapFps: fps }),
}))
