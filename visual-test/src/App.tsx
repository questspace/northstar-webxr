import { Canvas } from '@react-three/fiber'
import { useEffect, useState } from 'react'
import { useXR50 } from './hooks/useXR50'
import { useUltraleap } from './hooks/useUltraleap'
import { useCalibration } from './hooks/useCalibration'
import { DesktopPreview } from './components/DesktopPreview'
import { NorthStarRenderer } from './components/NorthStarRenderer'
import { HUD } from './components/HUD'

/** Initializes all sensor connections (rendered inside Canvas) */
function TrackingProvider() {
  useXR50()
  useUltraleap()
  return null
}

export function App() {
  const calibration = useCalibration()
  const [headset, setHeadset] = useState(window.location.hash === '#headset')
  const [hudVisible, setHudVisible] = useState(true)

  // Listen for hash changes
  useEffect(() => {
    const onHash = () => setHeadset(window.location.hash === '#headset')
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
          {!headset ? <HUD /> : <HeadsetHUD />}
        </div>
      )}
      <Canvas
        camera={{ position: [0, 2, 4], fov: 60 }}
        gl={{
          antialias: !headset,
          pixelRatio: 1,
          powerPreference: 'high-performance',
        }}
        style={{ background: '#000' }}
        frameloop="always"
      >
        <TrackingProvider />
        {headset && calibration ? (
          <NorthStarRenderer calibration={calibration} />
        ) : (
          <DesktopPreview />
        )}
      </Canvas>
    </>
  )
}

/** Minimal HUD for headset mode showing controls */
function HeadsetHUD() {
  return (
    <div style={{
      position: 'fixed', top: 10, left: 10, zIndex: 1000,
      color: '#0f0', fontFamily: 'monospace', fontSize: 11,
      background: 'rgba(0,0,0,0.7)', padding: 10, borderRadius: 4,
      pointerEvents: 'none', lineHeight: 1.6,
    }}>
      <div style={{ fontWeight: 600, marginBottom: 4 }}>VIBESTAR HEADSET</div>
      <div>F = fullscreen</div>
      <div>H = hide HUD</div>
      <div>U = toggle undistortion</div>
      <div>R = toggle re-projection</div>
      <div>M = toggle mirror</div>
    </div>
  )
}
