export interface CalibrationProfileSettings {
  headScaleX: number
  headScaleY: number
  headScaleZ: number
  headOffsetX: number
  headOffsetY: number
  headOffsetZ: number
  headRollScale: number
  headPitchScale: number
  headYawScale: number
  headRollOffset: number
  headPitchOffset: number
  headYawOffset: number
  handScaleX: number
  handScaleY: number
  handScaleZ: number
  handOffsetX: number
  handOffsetY: number
  handOffsetZ: number
  distortionStrength: number
  leftUvScale: number
  rightUvScale: number
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
  headScaleX: 1,
  headScaleY: 1,
  headScaleZ: 1,
  headOffsetX: 0,
  headOffsetY: 0,
  headOffsetZ: 0,
  headRollScale: 1,
  headPitchScale: 1,
  headYawScale: 1,
  headRollOffset: 0,
  headPitchOffset: 0,
  headYawOffset: 0,
  handScaleX: 1,
  handScaleY: 1,
  handScaleZ: 1,
  handOffsetX: 0,
  handOffsetY: 0,
  handOffsetZ: 0,
  distortionStrength: 1,
  leftUvScale: 1,
  rightUvScale: 1,
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
