/**
 * Fullscreen the page on the NorthStar panel. Chrome's Window Management API
 * (`getScreenDetails`, needs a user gesture + permission) lets us pick the screen
 * by its pixel size; without it we fall back to the current screen.
 */
interface ScreenDetailed { width: number; height: number; isPrimary: boolean; label: string }
interface ScreenDetails { screens: ScreenDetailed[]; currentScreen: ScreenDetailed }
type WindowWithScreens = Window & { getScreenDetails?: () => Promise<ScreenDetails> }

export const fullscreenOnHeadset = async (display: { width: number; height: number }) => {
  const details = await (window as WindowWithScreens).getScreenDetails?.().catch(() => undefined)
  const panel = details?.screens.find((s) => s.width === display.width && s.height === display.height)
    ?? details?.screens.find((s) => !s.isPrimary)
  if (details && !panel) {
    console.warn('[headset] no panel-sized screen found; screens:', details.screens.map((s) => `${s.label} ${s.width}x${s.height}`))
  }
  const options = panel ? ({ screen: panel } as unknown as FullscreenOptions) : undefined
  await document.documentElement.requestFullscreen(options)
}
