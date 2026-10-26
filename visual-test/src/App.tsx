import { Canvas } from '@react-three/fiber'
import { useEffect, useState } from 'react'
import { useXR50 } from './hooks/useXR50'
import { useUltraleap } from './hooks/useUltraleap'
import { useHeadsetSimulator } from './hooks/useHeadsetSimulator'
import { useCalibration } from './hooks/useCalibration'
import { DesktopPreview } from './components/DesktopPreview'
import { NorthStarRenderer } from './components/NorthStarRenderer'
import { XRHeadset } from './components/XRHeadset'
import { Scene } from './components/Scene'
import { HeadRig } from './components/HeadRig'
import { Hands } from './components/Hands'
import { HUD } from './components/HUD'
import { OpticsBench } from './components/OpticsBench'
import { useCalibrationProfilesStore } from './stores/calibrationProfiles'
import { PROFILE_DB_STORAGE_KEY } from './lib/profileDb'
import { fullscreenOnHeadset } from './lib/headsetDisplay'

const PANEL = { width: 2880, height: 1600 }

/** Initializes all sensor connections (rendered inside Canvas) */
function TrackingProvider({ enabled }: { enabled: boolean }) {
  // `?xr50=0` keeps the head at the origin: a fixed pose for comparing panel output.
  useXR50(enabled && new URLSearchParams(location.search).get('xr50') !== '0')
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
        else void fullscreenOnHeadset(calibration?.display ?? PANEL)
      }
      if (e.key === 'h' || e.key === 'H') {
        setHudVisible((v) => !v)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [calibration])

  return (
    <>
      {hudVisible && (
        <div id="hud-root">
          {mode === 'desktop' && <HUD />}
          {mode === 'headset' && <HeadsetHUD title="VIBESTAR HEADSET" display={calibration?.display ?? PANEL} />}
          {mode === 'simulator' && <SimulatorHUD />}
          {mode === 'optics' && <OpticsHUD />}
        </div>
      )}
      <Canvas
        camera={{ position: [0, 2, 4], fov: 60 }}
        dpr={1}
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
        ) : calibration && (mode === 'headset' ? (
          <XRHeadset calibration={calibration} />
        ) : mode === 'simulator' ? (
          <>
            <Scene />
            <HeadRig calibration={calibration}>
              <Hands calibration={calibration} />
            </HeadRig>
            <NorthStarRenderer calibration={calibration} />
          </>
        ) : (
          <DesktopPreview calibration={calibration} />
        ))}
      </Canvas>
    </>
  )
}

/** Minimal HUD for headset mode showing controls */
function HeadsetHUD({ title, display }: { title: string; display: { width: number; height: number } }) {
  return (
    <div style={{
      position: 'fixed', top: 10, left: 10, zIndex: 1000,
      color: '#0f0', fontFamily: 'monospace', fontSize: 11,
      background: 'rgba(0,0,0,0.7)', padding: 10, borderRadius: 4,
      pointerEvents: 'auto', lineHeight: 1.6,
    }}>
      <div style={{ fontWeight: 600, marginBottom: 4 }}>{title}</div>
      <div>WebXR immersive-ar session (additive)</div>
      <button
        onClick={() => void fullscreenOnHeadset(display)}
        style={{ margin: '4px 0 6px', padding: '4px 8px', font: 'inherit', color: '#000', background: '#0f0', border: 0, borderRadius: 3, cursor: 'pointer' }}
      >
        Show on headset display ({display.width}×{display.height})
      </button>
      <div>F = fullscreen on headset</div>
      <div>H = hide HUD</div>
      <div>U = toggle optics warp</div>
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
