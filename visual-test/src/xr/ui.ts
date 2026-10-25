/**
 * Hand-operated 3D UI for the interaction layer: canvas-text labels, push
 * buttons, a menu panel that can be summoned to the palm and left anywhere,
 * and sliders. Plain three.js objects; nothing here knows about the headset.
 */
import * as THREE from 'three'
import type { HandState, Interaction } from './interaction'

const TEXT_PX = 96
const CANVAS_HEIGHT = 128

export interface LabelOptions {
  /** Text height in meters */
  size?: number
  color?: string
  align?: 'left' | 'center' | 'right'
  weight?: string
}

export interface Label {
  group: THREE.Group
  set: (text: string) => void
}

/** A line of text on a transparent plane; `set` redraws it in place. */
export const createLabel = (text: string, { size = 0.025, color = '#ffffff', align = 'center', weight = '500' }: LabelOptions = {}): Label => {
  const canvas = document.createElement('canvas')
  canvas.height = CANVAS_HEIGHT
  const ctx = canvas.getContext('2d')!
  const font = `${weight} ${TEXT_PX}px system-ui, sans-serif`
  const texture = new THREE.CanvasTexture(canvas)
  texture.colorSpace = THREE.SRGBColorSpace
  texture.minFilter = THREE.LinearFilter
  const mesh = new THREE.Mesh(
    new THREE.PlaneGeometry(1, 1),
    new THREE.MeshBasicMaterial({ map: texture, transparent: true, depthWrite: false, side: THREE.DoubleSide }),
  )
  const group = new THREE.Group()
  group.add(mesh)

  const set = (value: string) => {
    ctx.font = font
    const width = Math.min(Math.ceil(ctx.measureText(value).width) + 8, 4096)
    // The canvas is as wide as the text (a resize clears it and resets the context).
    if (canvas.width !== Math.max(width, 64)) canvas.width = Math.max(width, 64)
    ctx.clearRect(0, 0, canvas.width, canvas.height)
    ctx.font = font
    ctx.textBaseline = 'middle'
    ctx.fillStyle = color
    ctx.fillText(value, 4, canvas.height / 2)
    texture.needsUpdate = true
    const w = (size * width) / canvas.height
    mesh.scale.set(w, size, 1)
    mesh.position.x = align === 'left' ? w / 2 : align === 'right' ? -w / 2 : 0
  }
  set(text)
  return { group, set }
}

export interface ButtonOptions {
  label: string
  width?: number
  height?: number
  onPress: () => void
  /** Reads the toggle state; the cap is lit while true. */
  toggled?: () => boolean
  color?: number
}

export interface Button {
  group: THREE.Group
  refresh: () => void
  dispose: () => void
}

const CAP_DEPTH = 0.012
const TRAVEL = 0.01

/** A push button: the fingertip presses the cap through its travel to click. */
export const createButton = (interaction: Interaction, { label, width = 0.09, height = 0.036, onPress, toggled, color = 0x2b6cb0 }: ButtonOptions): Button => {
  const group = new THREE.Group()
  const base = new THREE.Mesh(new THREE.BoxGeometry(width, height, 0.006), new THREE.MeshStandardMaterial({ color: 0x1a2230, roughness: 0.9 }))
  base.position.z = -0.003
  const capMaterial = new THREE.MeshStandardMaterial({ color, emissive: color, emissiveIntensity: 0.15, roughness: 0.6 })
  const cap = new THREE.Mesh(new THREE.BoxGeometry(width - 0.004, height - 0.004, CAP_DEPTH), capMaterial)
  const text = createLabel(label, { size: Math.min(height * 0.45, 0.016) })
  text.group.position.z = CAP_DEPTH / 2 + 0.0005
  cap.add(text.group)
  group.add(base, cap)

  let flash = 0
  const refresh = () => {
    const on = toggled?.() ?? false
    capMaterial.emissiveIntensity = Math.max(on ? 0.7 : 0.15, flash)
    capMaterial.color.set(on ? 0x38a169 : color)
    capMaterial.emissive.set(on ? 0x38a169 : color)
    flash = Math.max(0, flash - 0.08)
  }
  const item = interaction.add({
    object: cap,
    press: {
      size: [width, height],
      travel: TRAVEL,
      onPress: () => {
        flash = 1.2
        onPress()
      },
      onPush: (amount) => {
        cap.position.z = CAP_DEPTH / 2 - amount * TRAVEL
        refresh()
      },
    },
    onHover: (hovered) => { capMaterial.emissiveIntensity = hovered ? 0.5 : 0.15 },
  })
  cap.position.z = CAP_DEPTH / 2
  refresh()
  return { group, refresh, dispose: () => interaction.remove(item) }
}

export interface MenuItem {
  label: string
  onPress: () => void
  toggled?: () => boolean
}

export interface MenuOptions {
  title: string
  items: MenuItem[]
  columns?: number
  buttonWidth?: number
  buttonHeight?: number
}

export interface Menu {
  group: THREE.Group
  /** Per frame: button states, and the palm summon when `followHand` is set. */
  update: (camera: THREE.Camera) => void
  /** Hand whose upturned palm summons the menu; `none` leaves it world-locked. */
  followHand: 'left' | 'right' | 'none'
}

const GAP = 0.008
const PAD = 0.012

/** A panel of buttons with a grab handle. Faces +Z. */
export const createMenu = (interaction: Interaction, { title, items, columns = 2, buttonWidth = 0.09, buttonHeight = 0.036 }: MenuOptions): Menu => {
  const group = new THREE.Group()
  const rows = Math.ceil(items.length / columns)
  const width = columns * buttonWidth + (columns - 1) * GAP + 2 * PAD
  const titleHeight = 0.03
  const height = rows * buttonHeight + (rows - 1) * GAP + 2 * PAD + titleHeight

  const panel = new THREE.Mesh(
    new THREE.PlaneGeometry(width, height),
    new THREE.MeshBasicMaterial({ color: 0x0b1018, transparent: true, opacity: 0.75, side: THREE.DoubleSide }),
  )
  panel.position.z = -0.004
  const titleLabel = createLabel(title, { size: 0.018, color: '#9fd3ff', align: 'left' })
  titleLabel.group.position.set(-width / 2 + PAD, height / 2 - PAD - titleHeight / 2, 0.001)
  const handle = new THREE.Mesh(new THREE.BoxGeometry(width * 0.5, 0.012, 0.012), new THREE.MeshStandardMaterial({ color: 0x4a5568 }))
  handle.position.set(0, height / 2 + 0.012, 0)
  group.add(panel, titleLabel.group, handle)

  const buttons = items.map((item, i) => {
    const button = createButton(interaction, { label: item.label, width: buttonWidth, height: buttonHeight, onPress: item.onPress, toggled: item.toggled })
    const col = i % columns
    const row = Math.floor(i / columns)
    button.group.position.set(
      -width / 2 + PAD + buttonWidth / 2 + col * (buttonWidth + GAP),
      height / 2 - PAD - titleHeight - buttonHeight / 2 - row * (buttonHeight + GAP),
      0,
    )
    group.add(button.group)
    return button
  })

  let held = false
  interaction.add({
    object: handle,
    radius: 0.03,
    grab: { target: group, onStart: () => { held = true }, onEnd: () => { held = false } },
    onHover: (hovered) => { (handle.material as THREE.MeshStandardMaterial).color.set(hovered ? 0x90cdf4 : 0x4a5568) },
  })

  const target = new THREE.Vector3()
  const cameraPosition = new THREE.Vector3()
  const toCamera = new THREE.Vector3()
  const lookAt = new THREE.Matrix4()
  const desired = new THREE.Quaternion()
  const menu: Menu = {
    group,
    followHand: 'left',
    update: (camera) => {
      buttons.forEach((b) => b.refresh())
      const hand = interaction.hands.find((h) => h.handedness === menu.followHand)
      if (!hand?.tracked || held || hand.grab) return
      camera.getWorldPosition(cameraPosition)
      toCamera.subVectors(cameraPosition, hand.palm).normalize()
      // Palm turned toward the face: park the menu just above it, facing the viewer.
      if (hand.palmNormal.dot(toCamera) < 0.55) return
      target.copy(hand.palm).addScaledVector(hand.palmNormal, 0.06 + height / 2)
      group.position.lerp(target, 0.25)
      lookAt.lookAt(cameraPosition, group.position, THREE.Object3D.DEFAULT_UP)
      desired.setFromRotationMatrix(lookAt)
      group.quaternion.slerp(desired, 0.25)
    },
  }
  return menu
}

export interface SliderOptions {
  label: string
  min: number
  max: number
  value: number
  length?: number
  onChange: (value: number) => void
  format?: (value: number) => string
}

export interface Slider {
  group: THREE.Group
  set: (value: number) => void
  readonly value: number
}

/** A knob on a rail; pinch the knob and slide it. Lies along +X, faces +Z. */
export const createSlider = (interaction: Interaction, { label, min, max, value, length = 0.2, onChange, format = (v) => v.toFixed(2) }: SliderOptions): Slider => {
  const group = new THREE.Group()
  const rail = new THREE.Mesh(new THREE.BoxGeometry(length, 0.006, 0.006), new THREE.MeshStandardMaterial({ color: 0x4a5568 }))
  const knobMaterial = new THREE.MeshStandardMaterial({ color: 0x63b3ed, emissive: 0x63b3ed, emissiveIntensity: 0.2 })
  const knob = new THREE.Mesh(new THREE.SphereGeometry(0.012, 16, 12), knobMaterial)
  const text = createLabel('', { size: 0.016, align: 'left' })
  text.group.position.set(-length / 2, 0.028, 0)
  group.add(rail, knob, text.group)

  let current = value
  const toX = (v: number) => THREE.MathUtils.mapLinear(v, min, max, -length / 2, length / 2)
  const show = () => text.set(`${label}  ${format(current)}`)
  const set = (v: number) => {
    current = THREE.MathUtils.clamp(v, min, max)
    knob.position.set(toX(current), 0, 0)
    show()
  }
  interaction.add({
    object: knob,
    radius: 0.02,
    grab: {
      constrain: (local) => {
        local.x = THREE.MathUtils.clamp(local.x, -length / 2, length / 2)
        local.y = 0
        local.z = 0
      },
      onMove: () => {
        current = THREE.MathUtils.mapLinear(knob.position.x, -length / 2, length / 2, min, max)
        show()
        onChange(current)
      },
    },
    onHover: (hovered) => { knobMaterial.emissiveIntensity = hovered ? 0.8 : 0.2 },
  })
  set(value)
  return { group, set, get value() { return current } }
}

/** True while `hand` is pointing at nothing with its palm up: a cheap "summon" gesture helper. */
export const palmUp = (hand: HandState) => hand.tracked && hand.palmNormal.y > 0.7
