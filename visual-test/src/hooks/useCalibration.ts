import { useState, useEffect } from 'react'
import { loadCalibration, type NorthStarCalibration } from '../lib/calibration'

/**
 * Fetches and parses EskySettings.json into typed calibration data.
 * Returns null while loading.
 */
export const useCalibration = (): NorthStarCalibration | null => {
  const [calibration, setCalibration] = useState<NorthStarCalibration | null>(null)

  useEffect(() => {
    loadCalibration()
      .then(setCalibration)
      .catch((err) => console.error('[Calibration] failed to load:', err))
  }, [])

  return calibration
}
