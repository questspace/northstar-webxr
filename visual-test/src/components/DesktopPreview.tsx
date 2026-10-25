import { FirstPersonCalibrationView } from './FirstPersonCalibrationView'
import type { NorthStarCalibration } from '../lib/calibration'

/** Main desktop preview uses first-person XR50 camera motion. */
export function DesktopPreview({ calibration }: { calibration: NorthStarCalibration }) {
  return <FirstPersonCalibrationView calibration={calibration} />
}
