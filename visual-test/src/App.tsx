import { Canvas } from '@react-three/fiber'
import { useEffect, useState } from 'react'
import { useXR50 } from './hooks/useXR50'
import { useUltraleap } from './hooks/useUltraleap'
import { useHeadsetSimulator } from './hooks/useHeadsetSimulator'
import { useCalibration } from './hooks/useCalibration'
import { DesktopPreview } from './components/DesktopPreview'
import { NorthStarRenderer } from './components/NorthStarRenderer'
import { HUD } from './components/HUD'
import { OpticsBench } from './components/OpticsBench'
import { useCalibrationProfilesStore } from './stores/calibrationProfiles'
import { PROFILE_DB_STORAGE_KEY } from './lib/profileDb'

/** Initializes all sensor connections (rendered inside Canvas) */
function TrackingProvider({ enabled }: { enabled: boolean }) {
  useXR50(enabled)
  useUltraleap(enabled)
  return null
}

function SimulationProvider({ enabled }: { enabled: boolean }) {
  useHeadsetSimulator(enabled)
  return null
}

type AppMode = 'desktop' | 'headset' | 'simulator' | 'optics'

const getMode = (): AppMode => {
  if (window.location.hash === '#headset') return 'headset'
  if (window.location.hash === '#simulator') return 'simulator'
  if (window.location.hash === '#optics') return 'optics'
  return 'desktop'
}

export function App() {
  const calibration = useCalibration()
  const initProfiles = useCalibrationProfilesStore((s) => s.init)
  const reloadActiveFromDb = useCalibrationProfilesStore((s) => s.reloadActiveFromDb)
  const [mode, setMode] = useState<AppMode>(getMode)
  const [hudVisible, setHudVisible] = useState(true)
  const headsetOutput = mode === 'headset' || mode === 'simulator'
  const simulated = mode === 'simulator'

  // Listen for hash changes
  useEffect(() => {
    if (mode === 'simulator' || mode === 'optics') return
    void initProfiles()
  }, [initProfiles, mode])

  useEffect(() => {
    const onStorage = (e: StorageEvent) => {
      if (e.key === PROFILE_DB_STORAGE_KEY) void reloadActiveFromDb()
    }
    window.addEventListener('storage', onStorage)
    return () => window.removeEventListener('storage', onStorage)
  }, [reloadActiveFromDb])

  useEffect(() => {
    const onHash = () => setMode(getMode())
    window.addEventListener('hashchange', onHash)
    return () => window.removeEventListener('hashchange', onHash)
  }, [])

  // Keyboard shortcuts (global)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'f' || e.key === 'F') {
        if (document.fullscreenElement) document.exitFullscreen()
        else document.documentElement.requestFullscreen()
      }
      if (e.key === 'h' || e.key === 'H') {
        setHudVisible((v) => !v)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  return (
    <>
      {hudVisible && (
        <div id="hud-root">
          {mode === 'desktop' && <HUD />}
          {mode === 'headset' && <HeadsetHUD title="VIBESTAR HEADSET" />}
          {mode === 'simulator' && <SimulatorHUD />}
          {mode === 'optics' && <OpticsHUD />}
        </div>
      )}
      <Canvas
        camera={{ position: [0, 2, 4], fov: 60 }}
        gl={{
          antialias: !headsetOutput,
          pixelRatio: 1,
          powerPreference: 'high-performance',
        }}
        style={{ background: '#000' }}
        frameloop="always"
      >
        <TrackingProvider enabled={mode === 'desktop' || mode === 'headset'} />
        <SimulationProvider enabled={simulated} />
        {mode === 'optics' ? (
          <OpticsBench />
        ) : headsetOutput && calibration ? (
          <NorthStarRenderer
            calibration={calibration}
            outputMode={mode === 'simulator' ? 'viewer' : 'panel'}
          />
        ) : (
          <DesktopPreview />
        )}
      </Canvas>
    </>
  )
}

/** Minimal HUD for headset mode showing controls */
function HeadsetHUD({ title }: { title: string }) {
  return (
    <div style={{
      position: 'fixed', top: 10, left: 10, zIndex: 1000,
      color: '#0f0', fontFamily: 'monospace', fontSize: 11,
      background: 'rgba(0,0,0,0.7)', padding: 10, borderRadius: 4,
      pointerEvents: 'none', lineHeight: 1.6,
    }}>
      <div style={{ fontWeight: 600, marginBottom: 4 }}>{title}</div>
      <div>F = fullscreen</div>
      <div>H = hide HUD</div>
      <div>U = toggle undistortion</div>
      <div>R = toggle re-projection</div>
      <div>M = toggle mirror</div>
    </div>
  )
}

function SimulatorHUD() {
  return (
    <div style={{
      position: 'fixed', top: 10, left: 10, zIndex: 1000,
      color: '#dfffe8', fontFamily: 'monospace', fontSize: 11,
      background: 'rgba(0,0,0,0.78)', padding: 10, borderRadius: 4,
      pointerEvents: 'auto', lineHeight: 1.6, maxWidth: 330,
      border: '1px solid rgba(0,255,136,0.3)',
    }}>
      <div style={{ fontWeight: 700, color: '#00ff88', marginBottom: 4 }}>
        VIBESTAR HEADSET SIMULATOR
      </div>
      <div>Hardware sockets disabled</div>
      <div>Synthetic XR50 pose + Ultraleap hands</div>
      <div>Software wearer view after optics</div>
      <ModeLinks />
    </div>
  )
}

function OpticsHUD() {
  return (
    <div style={{
      position: 'fixed', top: 10, left: 10, zIndex: 1000,
      color: '#dbeafe', fontFamily: 'monospace', fontSize: 11,
      background: 'rgba(0,0,0,0.78)', padding: 10, borderRadius: 4,
      pointerEvents: 'auto', lineHeight: 1.6, maxWidth: 330,
      border: '1px solid rgba(102,204,255,0.3)',
    }}>
      <div style={{ fontWeight: 700, color: '#66ccff', marginBottom: 4 }}>
        NORTHSTAR OPTICS BENCH
      </div>
      <div>Approximate curved combiner model</div>
      <div>Eye points, display planes, reflection rays</div>
      <div>Drag to orbit, wheel to zoom</div>
      <ModeLinks />
    </div>
  )
}

function ModeLinks() {
  return (
    <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginTop: 8 }}>
      {[
        ['Preview', '#'],
        ['Headset', '#headset'],
        ['Simulator', '#simulator'],
        ['Optics', '#optics'],
      ].map(([label, href]) => (
        <a
          key={href}
          href={href}
          style={{ color: '#00ff88', textDecoration: 'none' }}
        >
          {label}
        </a>
      ))}
    </div>
  )
}
