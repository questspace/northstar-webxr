import { loadCalibration } from '../../lib/calibration'
import { installNorthStarXR } from '../runtime'
import { connectXR50 } from '../../hooks/useXR50'
import { connectUltraleap } from '../../hooks/useUltraleap'
import { useCalibrationProfilesStore } from '../../stores/calibrationProfiles'

/** Everything a plain WebXR page needs before it touches `navigator.xr`. */
export const bootNorthStarXR = async () => {
  const calibration = await loadCalibration()
  await useCalibrationProfilesStore.getState().init()
  installNorthStarXR(calibration)
  connectXR50()
  connectUltraleap()
  return calibration
}
