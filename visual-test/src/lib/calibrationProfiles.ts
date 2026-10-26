/**
 * Per-user tweaks layered on top of the EskySettings.json calibration.
 * Offsets only: SLAM and Leap are metric, so there is nothing to "scale".
 * Positions in meters (head frame), rotations in degrees.
 */
export interface CalibrationProfileSettings {
  headOffsetX: number
  headOffsetY: number
  headOffsetZ: number
  headRollOffset: number
  headPitchOffset: number
  headYawOffset: number
  handOffsetX: number
  handOffsetY: number
  handOffsetZ: number
  handRollOffset: number
  handPitchOffset: number
  handYawOffset: number
  /** 0 = raw eye images side by side, 1 = full optical warp */
  distortionStrength: number
  /** ±1: sense of the polynomial's horizontal / vertical ray axes (see xr/optics.ts) */
  rectSignX: number
  rectSignY: number
  /** Esky "eye_offset": additive eye-texture UV shift per eye */
  leftUvOffsetX: number
  leftUvOffsetY: number
  rightUvOffsetX: number
  rightUvOffsetY: number
}

export interface CalibrationProfileMeta {
  id: number
  userId: string
  name: string
  updatedAt: number
}

export const defaultCalibrationSettings = (): CalibrationProfileSettings => ({
  headOffsetX: 0,
  headOffsetY: 0,
  headOffsetZ: 0,
  headRollOffset: 0,
  headPitchOffset: 0,
  headYawOffset: 0,
  handOffsetX: 0,
  handOffsetY: 0,
  handOffsetZ: 0,
  handRollOffset: 0,
  handPitchOffset: 0,
  handYawOffset: 0,
  distortionStrength: 1,
  // Optics-only signs. Do not use them to fix tracking: an upside-down or mirrored world is a
  // frame constant in lib/transforms.ts or the Windows panel rotation (see PROTOCOL.md).
  rectSignX: 1,
  rectSignY: 1,
  leftUvOffsetX: 0,
  leftUvOffsetY: 0,
  rightUvOffsetX: 0,
  rightUvOffsetY: 0,
})

export const mergeCalibrationSettings = (
  partial: Partial<CalibrationProfileSettings> | undefined,
): CalibrationProfileSettings => ({
  ...defaultCalibrationSettings(),
  ...(partial ?? {}),
})
