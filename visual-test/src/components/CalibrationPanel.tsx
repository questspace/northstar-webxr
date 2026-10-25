import { useMemo, useRef, useState, type CSSProperties } from 'react'
import { useCalibrationProfilesStore } from '../stores/calibrationProfiles'
import {
  type CalibrationProfileSettings,
  mergeCalibrationSettings,
} from '../lib/calibrationProfiles'

type SettingKey = keyof CalibrationProfileSettings

interface SliderDef {
  key: SettingKey
  label: string
  min: number
  max: number
  step: number
}

const SLIDERS: SliderDef[] = [
  { key: 'headOffsetX', label: 'Head offset X', min: -2, max: 2, step: 0.001 },
  { key: 'headOffsetY', label: 'Head offset Y', min: -2, max: 2, step: 0.001 },
  { key: 'headOffsetZ', label: 'Head offset Z', min: -2, max: 2, step: 0.001 },
  { key: 'headPitchOffset', label: 'Head pitch offset', min: -180, max: 180, step: 0.1 },
  { key: 'headYawOffset', label: 'Head yaw offset', min: -180, max: 180, step: 0.1 },
  { key: 'headRollOffset', label: 'Head roll offset', min: -180, max: 180, step: 0.1 },
  { key: 'handOffsetX', label: 'Hand offset X', min: -0.5, max: 0.5, step: 0.001 },
  { key: 'handOffsetY', label: 'Hand offset Y', min: -0.5, max: 0.5, step: 0.001 },
  { key: 'handOffsetZ', label: 'Hand offset Z', min: -0.5, max: 0.5, step: 0.001 },
  { key: 'handPitchOffset', label: 'Hand pitch offset', min: -90, max: 90, step: 0.1 },
  { key: 'handYawOffset', label: 'Hand yaw offset', min: -90, max: 90, step: 0.1 },
  { key: 'handRollOffset', label: 'Hand roll offset', min: -90, max: 90, step: 0.1 },
  { key: 'distortionStrength', label: 'Distortion strength', min: 0, max: 1, step: 0.01 },
  { key: 'rectSignX', label: 'Warp horizontal sign', min: -1, max: 1, step: 2 },
  { key: 'rectSignY', label: 'Warp vertical sign', min: -1, max: 1, step: 2 },
  { key: 'leftUvOffsetX', label: 'Left UV offset X', min: -0.5, max: 0.5, step: 0.001 },
  { key: 'leftUvOffsetY', label: 'Left UV offset Y', min: -0.5, max: 0.5, step: 0.001 },
  { key: 'rightUvOffsetX', label: 'Right UV offset X', min: -0.5, max: 0.5, step: 0.001 },
  { key: 'rightUvOffsetY', label: 'Right UV offset Y', min: -0.5, max: 0.5, step: 0.001 },
]

export function CalibrationPanel({ floating = true }: { floating?: boolean }) {
  const [newProfileName, setNewProfileName] = useState('New profile')
  const [userIdInput, setUserIdInput] = useState('')
  const importInputRef = useRef<HTMLInputElement>(null)

  const userId = useCalibrationProfilesStore((s) => s.userId)
  const profiles = useCalibrationProfilesStore((s) => s.profiles)
  const activeProfileId = useCalibrationProfilesStore((s) => s.activeProfileId)
  const activeProfileName = useCalibrationProfilesStore((s) => s.activeProfileName)
  const settings = useCalibrationProfilesStore((s) => s.settings)
  const busy = useCalibrationProfilesStore((s) => s.busy)
  const setSetting = useCalibrationProfilesStore((s) => s.setSetting)
  const replaceSettings = useCalibrationProfilesStore((s) => s.replaceSettings)
  const setUserId = useCalibrationProfilesStore((s) => s.setUserId)
  const loadProfile = useCalibrationProfilesStore((s) => s.loadProfile)
  const saveCurrent = useCalibrationProfilesStore((s) => s.saveCurrent)
  const saveAsNew = useCalibrationProfilesStore((s) => s.saveAsNew)
  const deleteProfile = useCalibrationProfilesStore((s) => s.deleteProfile)
  const resetSettings = useCalibrationProfilesStore((s) => s.resetSettings)

  const selectedProfileValue = useMemo(
    () => (activeProfileId === null ? '' : String(activeProfileId)),
    [activeProfileId],
  )

  const exportJson = () => {
    const payload = {
      version: 1,
      userId,
      name: activeProfileName,
      settings,
    }
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `${activeProfileName || 'calibration'}.json`
    a.click()
    URL.revokeObjectURL(url)
  }

  const importJson = async (file: File) => {
    const raw = await file.text()
    const parsed = JSON.parse(raw) as {
      name?: string
      settings?: Partial<CalibrationProfileSettings>
    }
    const merged = mergeCalibrationSettings(parsed.settings)
    replaceSettings(merged)
    const name = parsed.name?.trim() || `Imported ${new Date().toISOString()}`
    await saveAsNew(name)
  }

  return (
    <div style={{
      position: floating ? 'fixed' : 'relative',
      right: floating ? 16 : undefined,
      top: floating ? 16 : undefined,
      width: 360,
      maxHeight: floating ? '92vh' : 'calc(100vh - 32px)',
      overflowY: 'auto',
      zIndex: 1200,
      color: '#ddd',
      background: 'rgba(10,10,10,0.92)',
      border: '1px solid rgba(0,255,136,0.25)',
      borderRadius: 8,
      padding: 12,
      fontFamily: '"SF Mono", "Fira Code", monospace',
      fontSize: 11,
      margin: floating ? 0 : '16px auto',
    }}>
      <div style={{ fontSize: 13, color: '#00ff88', marginBottom: 8, fontWeight: 700 }}>
        CALIBRATION PROFILES (SQLite)
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: '1fr auto', gap: 6, marginBottom: 8 }}>
        <input
          value={userIdInput}
          onChange={(e) => setUserIdInput(e.target.value)}
          placeholder={`user id (${userId})`}
          style={inputStyle}
        />
        <button
          style={buttonStyle}
          onClick={() => setUserId(userIdInput || userId)}
          disabled={busy}
        >
          Load User
        </button>
      </div>

      <div style={{ marginBottom: 8 }}>
        <div style={{ color: '#777', marginBottom: 4 }}>Active profile: {activeProfileName}</div>
        <select
          style={{ ...inputStyle, width: '100%' }}
          value={selectedProfileValue}
          onChange={(e) => loadProfile(Number(e.target.value))}
        >
          {profiles.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 6, marginBottom: 10 }}>
        <button style={buttonStyle} onClick={saveCurrent} disabled={busy}>Save Current</button>
        <button
          style={buttonStyle}
          onClick={() => {
            if (activeProfileId !== null) void deleteProfile(activeProfileId)
          }}
          disabled={busy || activeProfileId === null}
        >
          Delete Active
        </button>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: '1fr auto', gap: 6, marginBottom: 10 }}>
        <input
          value={newProfileName}
          onChange={(e) => setNewProfileName(e.target.value)}
          style={inputStyle}
        />
        <button style={buttonStyle} onClick={() => saveAsNew(newProfileName)} disabled={busy}>
          Save As
        </button>
      </div>

      <button style={{ ...buttonStyle, width: '100%', marginBottom: 10 }} onClick={resetSettings}>
        Reset Sliders
      </button>
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 6, marginBottom: 10 }}>
        <button style={buttonStyle} onClick={exportJson}>Export JSON</button>
        <button style={buttonStyle} onClick={() => importInputRef.current?.click()}>
          Import JSON
        </button>
      </div>
      <input
        ref={importInputRef}
        type="file"
        accept="application/json"
        style={{ display: 'none' }}
        onChange={(e) => {
          const file = e.target.files?.[0]
          if (!file) return
          void importJson(file)
          e.currentTarget.value = ''
        }}
      />

      {SLIDERS.map((slider) => {
        const value = settings[slider.key]
        return (
          <div key={slider.key} style={{ marginBottom: 8 }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 2 }}>
              <span>{slider.label}</span>
              <span style={{ color: '#00ff88' }}>{value.toFixed(3)}</span>
            </div>
            <input
              type="range"
              min={slider.min}
              max={slider.max}
              step={slider.step}
              value={value}
              onChange={(e) => setSetting(slider.key, Number(e.target.value))}
              style={{ width: '100%' }}
            />
          </div>
        )
      })}
    </div>
  )
}

const inputStyle: CSSProperties = {
  background: '#111',
  color: '#ddd',
  border: '1px solid #333',
  borderRadius: 4,
  padding: '6px 8px',
  fontSize: 11,
  fontFamily: '"SF Mono", "Fira Code", monospace',
}

const buttonStyle: CSSProperties = {
  background: '#1c1c1c',
  color: '#ddd',
  border: '1px solid #3a3a3a',
  borderRadius: 4,
  padding: '6px 8px',
  fontSize: 11,
  cursor: 'pointer',
}
