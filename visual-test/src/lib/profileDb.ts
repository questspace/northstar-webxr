import {
  type CalibrationProfileMeta,
  type CalibrationProfileSettings,
  mergeCalibrationSettings,
} from './calibrationProfiles'

export const PROFILE_DB_STORAGE_KEY = 'vibestar.calibration.profiles.v1'

interface StoredProfile extends CalibrationProfileMeta {
  settings: CalibrationProfileSettings
}

const readProfiles = (): StoredProfile[] => {
  try {
    const parsed = JSON.parse(localStorage.getItem(PROFILE_DB_STORAGE_KEY) ?? '[]') as StoredProfile[]
    return parsed.map((profile) => ({
      ...profile,
      settings: mergeCalibrationSettings(profile.settings),
    }))
  } catch {
    return []
  }
}

const writeProfiles = (profiles: StoredProfile[]) => {
  localStorage.setItem(PROFILE_DB_STORAGE_KEY, JSON.stringify(profiles))
}

const nextId = (profiles: StoredProfile[]) =>
  profiles.reduce((max, profile) => Math.max(max, profile.id), 0) + 1

export const listProfiles = async (userId: string): Promise<CalibrationProfileMeta[]> =>
  readProfiles()
    .filter((profile) => profile.userId === userId)
    .sort((a, b) => b.updatedAt - a.updatedAt || b.id - a.id)
    .map(({ settings: _settings, ...meta }) => meta)

export const getProfileSettings = async (
  id: number,
): Promise<CalibrationProfileSettings | null> =>
  readProfiles().find((profile) => profile.id === id)?.settings ?? null

export const upsertProfile = async (
  userId: string,
  name: string,
  settings: CalibrationProfileSettings,
  id?: number,
): Promise<number> => {
  const profiles = readProfiles()
  const now = Date.now()

  if (typeof id === 'number') {
    const index = profiles.findIndex((profile) => profile.id === id)
    if (index >= 0) {
      profiles[index] = { id, userId, name, updatedAt: now, settings }
      writeProfiles(profiles)
      return id
    }
  }

  const newId = nextId(profiles)
  profiles.push({ id: newId, userId, name, updatedAt: now, settings })
  writeProfiles(profiles)
  return newId
}

export const deleteProfile = async (id: number): Promise<void> => {
  writeProfiles(readProfiles().filter((profile) => profile.id !== id))
}
