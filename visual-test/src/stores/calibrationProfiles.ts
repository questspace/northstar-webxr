import { create } from 'zustand'
import {
  type CalibrationProfileMeta,
  type CalibrationProfileSettings,
  defaultCalibrationSettings,
} from '../lib/calibrationProfiles'
import * as profileDb from '../lib/profileDb'

interface CalibrationProfilesState {
  initialized: boolean
  userId: string
  profiles: CalibrationProfileMeta[]
  activeProfileId: number | null
  activeProfileName: string
  settings: CalibrationProfileSettings
  busy: boolean
  init: () => Promise<void>
  setUserId: (userId: string) => Promise<void>
  refreshProfiles: () => Promise<void>
  setSetting: <K extends keyof CalibrationProfileSettings>(
    key: K,
    value: CalibrationProfileSettings[K],
  ) => void
  replaceSettings: (settings: CalibrationProfileSettings) => void
  resetSettings: () => void
  loadProfile: (id: number) => Promise<void>
  saveAsNew: (name: string) => Promise<void>
  saveCurrent: () => Promise<void>
  deleteProfile: (id: number) => Promise<void>
  reloadActiveFromDb: () => Promise<void>
}

const DEFAULT_USER = 'default'
const DEFAULT_PROFILE_NAME = 'Default'

export const useCalibrationProfilesStore = create<CalibrationProfilesState>()((set, get) => ({
  initialized: false,
  userId: DEFAULT_USER,
  profiles: [],
  activeProfileId: null,
  activeProfileName: DEFAULT_PROFILE_NAME,
  settings: defaultCalibrationSettings(),
  busy: false,

  init: async () => {
    if (get().initialized) return
    set({ initialized: true, busy: true })
    try {
      const profiles = await profileDb.listProfiles(get().userId)
      if (profiles.length === 0) {
        const settings = defaultCalibrationSettings()
        const id = await profileDb.upsertProfile(
          get().userId,
          DEFAULT_PROFILE_NAME,
          settings,
        )
        const next = await profileDb.listProfiles(get().userId)
        set({
          profiles: next,
          activeProfileId: id,
          activeProfileName: DEFAULT_PROFILE_NAME,
          settings,
        })
        return
      }

      const first = profiles[0]
      const settings = (await profileDb.getProfileSettings(first.id)) ?? defaultCalibrationSettings()
      set({
        profiles,
        activeProfileId: first.id,
        activeProfileName: first.name,
        settings,
      })
    } finally {
      set({ busy: false })
    }
  },

  setUserId: async (userId: string) => {
    const nextUser = userId.trim() || DEFAULT_USER
    set({
      userId: nextUser,
      busy: true,
      activeProfileId: null,
      activeProfileName: DEFAULT_PROFILE_NAME,
      settings: defaultCalibrationSettings(),
    })
    try {
      const profiles = await profileDb.listProfiles(nextUser)
      if (profiles.length === 0) {
        const settings = defaultCalibrationSettings()
        const id = await profileDb.upsertProfile(nextUser, DEFAULT_PROFILE_NAME, settings)
        const refreshed = await profileDb.listProfiles(nextUser)
        set({
          profiles: refreshed,
          activeProfileId: id,
          activeProfileName: DEFAULT_PROFILE_NAME,
          settings,
        })
        return
      }

      const first = profiles[0]
      const settings = (await profileDb.getProfileSettings(first.id)) ?? defaultCalibrationSettings()
      set({
        profiles,
        activeProfileId: first.id,
        activeProfileName: first.name,
        settings,
      })
    } finally {
      set({ busy: false })
    }
  },

  refreshProfiles: async () => {
    const profiles = await profileDb.listProfiles(get().userId)
    set({ profiles })
  },

  setSetting: (key, value) =>
    set((state) => ({
      settings: {
        ...state.settings,
        [key]: value,
      },
    })),

  replaceSettings: (settings) => set({ settings }),

  resetSettings: () => set({ settings: defaultCalibrationSettings() }),

  loadProfile: async (id: number) => {
    const meta = get().profiles.find((p) => p.id === id)
    if (!meta) return
    set({ busy: true })
    try {
      const settings = await profileDb.getProfileSettings(id)
      if (!settings) return
      set({
        activeProfileId: id,
        activeProfileName: meta.name,
        settings,
      })
    } finally {
      set({ busy: false })
    }
  },

  saveAsNew: async (name: string) => {
    const trimmed = name.trim()
    if (!trimmed) return
    set({ busy: true })
    try {
      const id = await profileDb.upsertProfile(
        get().userId,
        trimmed,
        get().settings,
      )
      const profiles = await profileDb.listProfiles(get().userId)
      set({
        profiles,
        activeProfileId: id,
        activeProfileName: trimmed,
      })
    } finally {
      set({ busy: false })
    }
  },

  saveCurrent: async () => {
    set({ busy: true })
    try {
      const activeId = get().activeProfileId
      const activeName = get().activeProfileName.trim() || DEFAULT_PROFILE_NAME
      const id = await profileDb.upsertProfile(
        get().userId,
        activeName,
        get().settings,
        activeId ?? undefined,
      )
      const profiles = await profileDb.listProfiles(get().userId)
      set({
        profiles,
        activeProfileId: id,
        activeProfileName: activeName,
      })
    } finally {
      set({ busy: false })
    }
  },

  deleteProfile: async (id: number) => {
    set({ busy: true })
    try {
      await profileDb.deleteProfile(id)
      const profiles = await profileDb.listProfiles(get().userId)
      if (profiles.length === 0) {
        const settings = defaultCalibrationSettings()
        const newId = await profileDb.upsertProfile(
          get().userId,
          DEFAULT_PROFILE_NAME,
          settings,
        )
        const refreshed = await profileDb.listProfiles(get().userId)
        set({
          profiles: refreshed,
          activeProfileId: newId,
          activeProfileName: DEFAULT_PROFILE_NAME,
          settings,
        })
        return
      }
      const first = profiles[0]
      const settings = (await profileDb.getProfileSettings(first.id)) ?? defaultCalibrationSettings()
      set({
        profiles,
        activeProfileId: first.id,
        activeProfileName: first.name,
        settings,
      })
    } finally {
      set({ busy: false })
    }
  },

  reloadActiveFromDb: async () => {
    const activeId = get().activeProfileId
    if (activeId === null) return
    const profiles = await profileDb.listProfiles(get().userId)
    const settings = await profileDb.getProfileSettings(activeId)
    if (!settings) return
    const meta = profiles.find((p) => p.id === activeId)
    set({
      profiles,
      settings,
      activeProfileName: meta?.name ?? get().activeProfileName,
    })
  },
}))
