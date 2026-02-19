import { useState, useEffect } from 'react'
import { parseCalibration, type NorthStarCalibration } from '../lib/calibration'

const CALIBRATION_URL = '/calibration/EskySettings.json'

/**
 * Fetches and parses EskySettings.json into typed calibration data.
 * Returns null while loading.
 */
export const useCalibration = (): NorthStarCalibration | null => {
  const [calibration, setCalibration] = useState<NorthStarCalibration | null>(null)

  useEffect(() => {
    fetch(CALIBRATION_URL)
      .then((r) => r.json())
      .then((json) => {
        const cal = parseCalibration(json)
        console.log('[Calibration] loaded:', {
          leftEyePos: cal.leftEye.eyePosition.toArray(),
          rightEyePos: cal.rightEye.eyePosition.toArray(),
          leftFrustum: cal.leftEye.frustum,
          leapOffset: cal.leapTranslation.toArray(),
          display: cal.display,
          polyCoeffs: cal.leftEye.uvToRectX.length + cal.leftEye.uvToRectY.length +
            cal.rightEye.uvToRectX.length + cal.rightEye.uvToRectY.length,
        })
        setCalibration(cal)
      })
      .catch((err) => console.error('[Calibration] failed to load:', err))
  }, [])

  return calibration
}
