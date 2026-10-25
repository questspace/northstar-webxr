import { useEffect } from 'react'
import { useThree } from '@react-three/fiber'
import type { NorthStarCalibration } from '../lib/calibration'
import { installNorthStarXR } from '../xr/runtime'
import { Scene } from './Scene'
import { HeadRig } from './HeadRig'
import { Hands } from './Hands'

/**
 * Headset mode as a plain WebXR app: installs the NorthStar runtime, opens an
 * immersive-ar session on three's renderer and lets it drive the cameras.
 * The real hands are visible through the combiners, so only occluders are drawn.
 */
export function XRHeadset({ calibration }: { calibration: NorthStarCalibration }) {
  const gl = useThree((s) => s.gl)

  useEffect(() => {
    const xr = installNorthStarXR(calibration)
    gl.xr.enabled = true
    gl.domElement.style.transform = 'none'
    let session: XRSession | null = null
    let cancelled = false

    navigator.xr!
      .requestSession('immersive-ar', { requiredFeatures: ['local-floor', 'hand-tracking'], optionalFeatures: ['anchors'] })
      .then(async (s) => {
        if (cancelled) return s.end()
        session = s
        await gl.xr.setSession(s)
      })
      .catch((err) => console.error('[XR] session failed:', err))

    const onKey = (e: KeyboardEvent) => {
      const key = e.key.toLowerCase()
      if (key === 'u') xr.warpEnabled = !xr.warpEnabled
      if (key === 'r') xr.reprojectionEnabled = !xr.reprojectionEnabled
      if (key === 'm') {
        const mirrored = gl.domElement.style.transform === 'scaleX(-1)'
        gl.domElement.style.transform = mirrored ? 'none' : 'scaleX(-1)'
      }
    }
    window.addEventListener('keydown', onKey)

    return () => {
      cancelled = true
      window.removeEventListener('keydown', onKey)
      session?.end().catch(() => {})
      gl.xr.enabled = false
    }
  }, [gl, calibration])

  return (
    <>
      <Scene />
      <HeadRig calibration={calibration}>
        <Hands calibration={calibration} occluder />
      </HeadRig>
    </>
  )
}
