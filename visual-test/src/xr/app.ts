/**
 * The boilerplate every hand-driven WebXR page on this stack shares: runtime
 * boot, renderer, immersive-ar button, interaction layer, hand occluders, an
 * orbit camera for the desktop preview, keys (U warp, R re-projection, O
 * occluders) and `?warp=0` / `?hand=mouse` / `?xr50=0` dev switches.
 */
import * as THREE from 'three'
import { ARButton } from 'three/examples/jsm/webxr/ARButton.js'
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'
import { bootNorthStarXR } from './examples/boot'
import { createInteraction, type Interaction } from './interaction'
import { createHandOccluders } from './hands'

export interface XRApp {
  renderer: THREE.WebGLRenderer
  scene: THREE.Scene
  camera: THREE.PerspectiveCamera
  interaction: Interaction
  occluders: ReturnType<typeof createHandOccluders>
  /** Reference space of the running session (local-floor), when there is one. */
  referenceSpace: () => XRReferenceSpace | null
  /** The XRFrame being processed right now (valid only during this frame's callbacks). */
  frame: () => XRFrame | undefined
  /** Registers a per-frame callback: seconds since the last frame, time, and the XRFrame in a session. */
  onFrame: (fn: FrameCallback) => void
  onSessionStart: (fn: (session: XRSession) => void) => void
}

export type FrameCallback = (dt: number, time: number, frame: XRFrame | undefined) => void

export interface XRAppOptions {
  /** Desktop preview camera placement */
  lookFrom?: THREE.Vector3Like
  lookAt?: THREE.Vector3Like
  /** Draw occluders visibly (desktop debugging) */
  showHands?: boolean
}

export const query = new URLSearchParams(location.search)

/** ARButton switches the session to the `local` space (origin at the head); scenes here are floor-based. */
export const keepFloorSpace = (renderer: THREE.WebGLRenderer) => {
  const setSession = renderer.xr.setSession.bind(renderer.xr)
  renderer.xr.setSession = (session) => {
    renderer.xr.setReferenceSpaceType('local-floor')
    return setSession(session)
  }
}

export const SESSION_INIT = { requiredFeatures: ['hand-tracking', 'local-floor'], optionalFeatures: ['anchors'] }

/**
 * On the headset nobody can click START AR: the NorthStar runtime needs no user
 * gesture, so the session starts on load (`?autostart=0` keeps the button only).
 */
export const autoStartSession = (renderer: THREE.WebGLRenderer) => {
  if (query.get('autostart') === '0' || !navigator.userAgent.includes('NorthStar/IWER')) return
  navigator.xr!.requestSession('immersive-ar', SESSION_INIT)
    .then((session) => renderer.xr.setSession(session))
    .catch((err) => console.warn('[XR] auto start failed:', err))
}

export const createXRApp = async ({ lookFrom = { x: 0.5, y: 1.5, z: 0.5 }, lookAt = { x: 0, y: 1.0, z: -0.7 }, showHands = query.get('hands') === 'show' }: XRAppOptions = {}): Promise<XRApp> => {
  const xr = await bootNorthStarXR()
  if (query.get('warp') === '0') xr.warpEnabled = false

  const scene = new THREE.Scene()
  const camera = new THREE.PerspectiveCamera(60, window.innerWidth / window.innerHeight, 0.01, 50)
  camera.position.copy(lookFrom)
  scene.add(new THREE.HemisphereLight(0xffffff, 0x334466, 2.2))
  const sun = new THREE.DirectionalLight(0xffffff, 1.2)
  sun.position.set(1, 3, 2)
  scene.add(sun)

  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true })
  renderer.setPixelRatio(1)
  renderer.setSize(window.innerWidth, window.innerHeight)
  renderer.xr.enabled = true
  document.body.appendChild(renderer.domElement)
  document.body.appendChild(ARButton.createButton(renderer, SESSION_INIT))
  keepFloorSpace(renderer)
  autoStartSession(renderer)

  const controls = new OrbitControls(camera, renderer.domElement)
  controls.target.copy(lookAt)
  controls.update()

  const interaction = createInteraction(renderer, scene)
  const occluders = createHandOccluders(renderer, scene, { visible: showHands })

  let referenceSpace: XRReferenceSpace | null = null
  const sessionStart: ((session: XRSession) => void)[] = []
  renderer.xr.addEventListener('sessionstart', () => {
    const session = renderer.xr.getSession()!
    referenceSpace = renderer.xr.getReferenceSpace()
    sessionStart.forEach((fn) => fn(session))
  })
  renderer.xr.addEventListener('sessionend', () => { referenceSpace = null })

  const callbacks: FrameCallback[] = []
  let last = 0
  let current: XRFrame | undefined
  renderer.setAnimationLoop((time, frame) => {
    const dt = Math.min((time - (last || time)) / 1000, 0.1)
    last = time
    current = frame
    interaction.update()
    occluders.update()
    for (const fn of callbacks) fn(dt, time / 1000, frame)
    renderer.render(scene, camera)
  })

  window.addEventListener('resize', () => {
    camera.aspect = window.innerWidth / window.innerHeight
    camera.updateProjectionMatrix()
    renderer.setSize(window.innerWidth, window.innerHeight)
  })
  window.addEventListener('keydown', (e) => {
    const key = e.key.toLowerCase()
    if (key === 'u') xr.warpEnabled = !xr.warpEnabled
    if (key === 'r') xr.reprojectionEnabled = !xr.reprojectionEnabled
    if (key === 'o') occluders.enabled = !occluders.enabled
  })

  return {
    renderer,
    scene,
    camera,
    interaction,
    occluders,
    referenceSpace: () => referenceSpace,
    frame: () => current,
    onFrame: (fn) => callbacks.push(fn),
    onSessionStart: (fn) => sessionStart.push(fn),
  }
}
