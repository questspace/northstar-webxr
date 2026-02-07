import { useTrackingStore } from '../stores/tracking'

/** HTML overlay HUD showing connection states and telemetry */
export function HUD() {
  const xr50 = useTrackingStore((s) => s.xr50Connected)
  const leap = useTrackingStore((s) => s.leapConnected)
  const pose = useTrackingStore((s) => s.headPose)
  const xr50Fps = useTrackingStore((s) => s.xr50Fps)
  const leapFps = useTrackingStore((s) => s.leapFps)
  const leftHand = useTrackingStore((s) => s.leftHand)
  const rightHand = useTrackingStore((s) => s.rightHand)

  return (
    <div style={{
      position: 'fixed', top: 16, left: 16, zIndex: 1000,
      background: 'rgba(0,0,0,0.85)', color: '#e0e0e0',
      fontFamily: '"SF Mono", "Fira Code", monospace', fontSize: 12,
      padding: 16, borderRadius: 8, border: '1px solid rgba(0,255,136,0.2)',
      minWidth: 260, lineHeight: 1.6,
    }}>
      <div style={{ fontSize: 14, fontWeight: 600, color: '#00ff88', marginBottom: 8, letterSpacing: 1 }}>
        VIBESTAR
      </div>

      <Indicator on={xr50} label={`XR50 6DOF ${xr50 ? `(${xr50Fps} Hz)` : ''}`} />
      <Indicator on={leap} label={`Ultraleap ${leap ? `(${leapFps} FPS)` : ''}`} />

      {xr50 && (
        <div style={{ marginTop: 8, paddingTop: 8, borderTop: '1px solid rgba(255,255,255,0.1)' }}>
          <Row label="Pos" value={`${pose.x.toFixed(3)}, ${pose.y.toFixed(3)}, ${pose.z.toFixed(3)}`} />
          <Row label="Rot" value={`${pose.roll.toFixed(1)}° ${pose.pitch.toFixed(1)}° ${pose.yaw.toFixed(1)}°`} />
        </div>
      )}

      {leap && (
        <div style={{ marginTop: 4 }}>
          <Row label="Hands" value={`L: ${leftHand ? '✓' : '—'}  R: ${rightHand ? '✓' : '—'}`} />
        </div>
      )}
    </div>
  )
}

function Indicator({ on, label }: { on: boolean; label: string }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
      <span style={{
        width: 8, height: 8, borderRadius: '50%', flexShrink: 0,
        background: on ? '#00ff88' : '#ff4444',
        boxShadow: on ? '0 0 8px #00ff88' : 'none',
      }} />
      <span>{label}</span>
    </div>
  )
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div style={{ display: 'flex', justifyContent: 'space-between', gap: 12 }}>
      <span style={{ color: '#666', textTransform: 'uppercase', fontSize: 10 }}>{label}</span>
      <span style={{ color: '#00ff88', fontVariantNumeric: 'tabular-nums' }}>{value}</span>
    </div>
  )
}
