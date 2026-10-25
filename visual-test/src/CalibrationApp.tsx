import { useEffect } from 'react'
import { CalibrationPanel } from './components/CalibrationPanel'
import { useCalibrationProfilesStore } from './stores/calibrationProfiles'

export function CalibrationApp() {
  const initProfiles = useCalibrationProfilesStore((s) => s.init)

  useEffect(() => {
    void initProfiles()
  }, [initProfiles])

  return (
    <div style={{ minHeight: '100vh', background: '#050505', color: '#ddd' }}>
      <div style={{
        padding: '12px 16px',
        borderBottom: '1px solid #1b1b1b',
        fontFamily: '"SF Mono", "Fira Code", monospace',
      }}>
        <div style={{ color: '#00ff88', fontWeight: 700 }}>VIBESTAR CALIBRATION CONSOLE</div>
        <div style={{ color: '#8a8a8a', marginTop: 4, fontSize: 12 }}>
          2D-only profile editor. Save profile changes here and the main app tab picks them up live.
        </div>
      </div>
      <CalibrationPanel floating={false} />
    </div>
  )
}
