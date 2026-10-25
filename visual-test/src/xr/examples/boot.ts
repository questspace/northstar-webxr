import { loadCalibration } from '../../lib/calibration'
import { installNorthStarXR } from '../runtime'
import { connectXR50 } from '../../hooks/useXR50'
import { connectUltraleap } from '../../hooks/useUltraleap'
import { installDevHand } from '../devhand'
import { useCalibrationProfilesStore } from '../../stores/calibrationProfiles'
import { useTrackingStore } from '../../stores/tracking'
import * as THREE from 'three'

/**
 * Everything a plain WebXR page needs before it touches `navigator.xr`.
 * Dev switches: `?hand=mouse` drives a synthetic hand with the mouse instead of
 * the Leap, `?xr50=0` leaves the head at the origin instead of waiting for the bridge
 * (`?pitch=-40` then tilts it down to look at a bench).
 */
export const bootNorthStarXR = async () => {
  const query = new URLSearchParams(location.search)
  const calibration = await loadCalibration()
  await useCalibrationProfilesStore.getState().init()
  const xr = installNorthStarXR(calibration)
  if (query.get('xr50') !== '0') connectXR50()
  else {
    const pitch = THREE.MathUtils.degToRad(Number(query.get('pitch') ?? 0))
    const quaternion = new THREE.Quaternion().setFromEuler(new THREE.Euler(pitch, 0, 0)).toArray() as [number, number, number, number]
    useTrackingStore.getState().setHeadPose({ position: [0, 0, 0], quaternion, confidence: 1, timestamp: 0, receivedAt: 0 })
  }
  if (query.get('hand') === 'mouse') installDevHand(calibration)
  else connectUltraleap()
  return xr
}
