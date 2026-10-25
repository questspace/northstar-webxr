/**
 * Seeing Spaces workbench on NorthStar. Hands: poke the robot's side to push
 * it, pinch the scrub knob, press the menu (left palm up summons it), slide the
 * gains, touch a sweep cell. Keys on the desktop: Space pause, P / Shift+P push,
 * [ ] scrub, I pin, K reset, N noise.
 */
import * as THREE from 'three'
import { createXRApp } from '../app'
import { createWorkbench } from '../../workbench'

const main = async () => {
  const app = await createXRApp({ lookFrom: { x: 0.7, y: 1.45, z: 0.2 }, lookAt: { x: 0, y: 1.05, z: -0.75 } })
  app.scene.add(new THREE.GridHelper(10, 20, 0x335, 0x224))

  const bench = createWorkbench(app)
  app.scene.add(bench.group)
  Object.assign(window, { bench, app })

  const viewer = () => (app.renderer.xr.isPresenting ? app.renderer.xr.getCamera() : app.camera)
  app.onFrame((dt) => bench.update(dt, viewer()))

  let offset = 0
  window.addEventListener('keydown', (e) => {
    const actions: Record<string, () => void> = {
      ' ': () => bench.pause(),
      p: () => bench.push(e.shiftKey ? -1 : 1),
      '[': () => bench.scrub((offset = Math.max(-10, offset - 0.25))),
      ']': () => bench.scrub((offset = Math.min(0, offset + 0.25))),
      i: bench.pin,
      k: bench.reset,
      n: () => { bench.params.noise = !bench.params.noise },
    }
    actions[e.key.toLowerCase()]?.()
    if (!bench.paused) offset = 0
  })
}

void main()
