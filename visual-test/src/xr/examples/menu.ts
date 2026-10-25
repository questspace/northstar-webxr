/**
 * Hands as the interface: a menu summoned by turning the left palm toward you
 * (grab its handle to park it anywhere), objects spawned from it that you
 * pinch to move and turn, hold with both hands to scale, and that come back
 * where you left them after a reload (WebXR anchors). Real hands occlude them.
 */
import * as THREE from 'three'
import { createXRApp } from '../app'
import { createMenu, createLabel } from '../ui'
import { createPlacements, type Placed } from '../anchors'

interface ObjectData { color: number }

const GEOMETRY: Record<string, () => THREE.BufferGeometry> = {
  cube: () => new THREE.BoxGeometry(0.08, 0.08, 0.08),
  sphere: () => new THREE.SphereGeometry(0.045, 24, 16),
  cone: () => new THREE.ConeGeometry(0.045, 0.1, 24),
  torus: () => new THREE.TorusGeometry(0.04, 0.015, 12, 32),
}
const COLORS = [0x63b3ed, 0xf6ad55, 0x68d391, 0xfc8181, 0xb794f4, 0xfaf089]

const main = async () => {
  const app = await createXRApp()
  const { scene, interaction } = app
  scene.add(new THREE.GridHelper(10, 20, 0x335, 0x224))

  // Anchors can only be made inside the frame callback, so this is read when needed, never cached.
  const xr = () => {
    const frame = app.frame()
    const space = app.referenceSpace()
    return frame && space ? { frame, space } : null
  }
  const placedOf = (object: THREE.Object3D) => [...placements.items.values()].find((p) => p.object === object)!

  const placements = createPlacements<ObjectData>('northstar.menu.objects', (kind, data) => {
    const material = new THREE.MeshStandardMaterial({ color: data.color, emissive: data.color, emissiveIntensity: 0.12, roughness: 0.5 })
    const mesh = new THREE.Mesh(GEOMETRY[kind](), material)
    interaction.add({
      object: mesh,
      grab: {
        onStart: () => { placedOf(mesh).held = true },
        onEnd: () => {
          const placed = placedOf(mesh)
          placed.held = interaction.holder(mesh) !== undefined
          const now = xr()
          if (!placed.held && now) void placements.move(now.frame, now.space, placed)
        },
      },
      onHover: (hovered) => { material.emissiveIntensity = hovered ? 0.5 : 0.12 },
    })
    return mesh
  }, scene)

  const viewer = () => (app.renderer.xr.isPresenting ? app.renderer.xr.getCamera() : app.camera)
  const spawn = (kind: string) => {
    const camera = viewer()
    const matrix = new THREE.Matrix4().compose(
      new THREE.Vector3(0, -0.1, -0.45).applyMatrix4(camera.matrixWorld),
      camera.getWorldQuaternion(new THREE.Quaternion()),
      new THREE.Vector3(1, 1, 1),
    )
    const data = { color: COLORS[placements.items.size % COLORS.length] }
    const now = xr()
    if (now) void placements.place(now.frame, now.space, kind, data, matrix)
    else {
      // Desktop preview without a session: unanchored object, not stored.
      const object = new THREE.Mesh(GEOMETRY[kind](), new THREE.MeshStandardMaterial({ color: data.color }))
      matrix.decompose(object.position, object.quaternion, object.scale)
      scene.add(object)
      interaction.add({ object, grab: true })
    }
  }

  const menu = createMenu(interaction, {
    title: 'Objects',
    items: [
      ...Object.keys(GEOMETRY).map((kind) => ({ label: kind[0].toUpperCase() + kind.slice(1), onPress: () => spawn(kind) })),
      { label: 'Clear all', onPress: () => placements.clear() },
      { label: 'Occluders', onPress: () => { app.occluders.enabled = !app.occluders.enabled }, toggled: () => app.occluders.enabled },
    ],
  })
  menu.group.position.set(-0.25, 1.25, -0.5)
  menu.group.rotation.y = 0.4
  scene.add(menu.group)

  const status = createLabel('', { size: 0.02, color: '#9fd3ff' })
  status.group.position.set(0, 1.55, -0.8)
  scene.add(status.group)

  Object.assign(window, { app, placements })

  let restored = false
  app.onFrame((_dt, _time, frame) => {
    const space = app.referenceSpace()
    if (frame && space) {
      if (!restored) {
        restored = true
        void placements.restore(frame.session, frame, space)
      }
      placements.update(frame, space)
    }
    menu.update(viewer())
    const hands = interaction.hands.filter((h) => h.tracked).map((h) => `${h.handedness}${h.pinching ? ' pinch' : ''}`)
    status.set(`${placements.items.size} anchored objects · hands: ${hands.join(', ') || 'none'} · palm up (left) summons the menu`)
  })
}

void main()
