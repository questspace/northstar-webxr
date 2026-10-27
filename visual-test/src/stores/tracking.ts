import { create } from 'zustand'
import type { MeshBlock, WorldPlane } from '../lib/transforms'

export type Vec3 = [number, number, number]
/** [qx, qy, qz, qw] */
export type Quat = [number, number, number, number]

/** Tracker pose in the Three.js world frame (meters, Y up, Z back). */
export interface HeadPose {
  position: Vec3
  quaternion: Quat
  /** SLAM confidence [0..1]; 0 = IMU-only orientation, position is stale */
  confidence: number
  timestamp: number
  /** Wall-clock ms (Date.now() domain) when the sample left the tracker; for latency stats */
  receivedAt: number
}

/** One tracked hand in the Leap sensor frame (mm). */
export interface HandData {
  id: number
  type: 'left' | 'right'
  palmPosition: Vec3
  /** Unit vector out of the palm */
  palmNormal: Vec3
  /** Unit vector from palm to fingers */
  direction: Vec3
  wrist: Vec3
  pinchStrength: number
  /** thumb..pinky, each [carp, mcp, pip, dip, tip] */
  fingers: Vec3[][]
  /** finger widths in mm, thumb..pinky */
  fingerWidth: number[]
}

export interface TrackingState {
  // Connection states
  xr50Connected: boolean
  xr50Stalled: boolean
  leapConnected: boolean
  xr50Status: string
  leapStatus: string

  // Head tracking (filtered)
  headPose: HeadPose

  // Hand tracking
  leftHand: HandData | null
  rightHand: HandData | null

  // Surfaces detected by the XR50 (bridge SDK source only), Three.js world
  planes: WorldPlane[]
  // Room mesh by block (bridge recon source), Three.js world
  meshBlocks: Map<string, MeshBlock>

  // Telemetry
  xr50Fps: number
  leapFps: number

  // Actions
  setXR50Connected: (connected: boolean) => void
  setXR50Stalled: (stalled: boolean) => void
  setLeapConnected: (connected: boolean) => void
  setXR50Status: (status: string) => void
  setLeapStatus: (status: string) => void
  setHeadPose: (pose: HeadPose) => void
  setHands: (left: HandData | null, right: HandData | null) => void
  setPlanes: (planes: WorldPlane[]) => void
  /** Replaces the given blocks; blocks without triangles are removed. */
  updateMeshBlocks: (blocks: MeshBlock[]) => void
  clearEnvironment: () => void
  setXR50Fps: (fps: number) => void
  setLeapFps: (fps: number) => void
}

export const useTrackingStore = create<TrackingState>()((set) => ({
  xr50Connected: false,
  xr50Stalled: false,
  leapConnected: false,
  xr50Status: 'Waiting for XR50 bridge on localhost:8080',
  leapStatus: 'Waiting for Ultraleap websocket on localhost:6437',
  headPose: { position: [0, 0, 0], quaternion: [0, 0, 0, 1], confidence: 0, timestamp: 0, receivedAt: 0 },
  leftHand: null,
  rightHand: null,
  planes: [],
  meshBlocks: new Map(),
  xr50Fps: 0,
  leapFps: 0,

  setXR50Connected: (connected) => set({ xr50Connected: connected }),
  setXR50Stalled: (stalled) => set({ xr50Stalled: stalled }),
  setLeapConnected: (connected) => set({ leapConnected: connected }),
  setXR50Status: (status) => set({ xr50Status: status }),
  setLeapStatus: (status) => set({ leapStatus: status }),
  setHeadPose: (pose) => set({ headPose: pose }),
  setHands: (left, right) => set({ leftHand: left, rightHand: right }),
  setPlanes: (planes) => set({ planes }),
  updateMeshBlocks: (blocks) => set((s) => {
    const meshBlocks = new Map(s.meshBlocks)
    for (const b of blocks) {
      if (b.indices.length) meshBlocks.set(b.key, b)
      else meshBlocks.delete(b.key)
    }
    return { meshBlocks }
  }),
  clearEnvironment: () => set({ planes: [], meshBlocks: new Map() }),
  setXR50Fps: (fps) => set({ xr50Fps: fps }),
  setLeapFps: (fps) => set({ leapFps: fps }),
}))
