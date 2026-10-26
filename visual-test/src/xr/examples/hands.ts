/**
 * three.js hand input on NorthStar: XRHandModelFactory joints from the Leap,
 * a floor grid for the world-lock check and a row of spinning cubes.
 */
import * as THREE from 'three'
import { ARButton } from 'three/examples/jsm/webxr/ARButton.js'
import { XRHandModelFactory } from 'three/examples/jsm/webxr/XRHandModelFactory.js'
import { bootNorthStarXR } from './boot'
import { keepFloorSpace } from '../app'

const main = async () => {
  await bootNorthStarXR()

  const scene = new THREE.Scene()
  const camera = new THREE.PerspectiveCamera(70, window.innerWidth / window.innerHeight, 0.01, 20)
  scene.add(new THREE.HemisphereLight(0xffffff, 0x444466, 3))
  scene.add(new THREE.GridHelper(20, 40, 0x4444aa, 0x222255))

  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true })
  renderer.setPixelRatio(1)
  renderer.setSize(window.innerWidth, window.innerHeight)
  renderer.xr.enabled = true
  document.body.appendChild(renderer.domElement)
  document.body.appendChild(ARButton.createButton(renderer, { requiredFeatures: ['hand-tracking'] }))
  keepFloorSpace(renderer)

  const handModelFactory = new XRHandModelFactory()
  for (const i of [0, 1]) {
    const hand = renderer.xr.getHand(i)
    hand.add(handModelFactory.createHandModel(hand, 'boxes'))
    scene.add(hand)
  }

  const cubes = Array.from({ length: 6 }, (_, i) => {
    const cube = new THREE.Mesh(
      new THREE.BoxGeometry(0.08, 0.08, 0.08),
      new THREE.MeshPhongMaterial({ color: 0xff8800, emissive: 0x442200 }),
    )
    cube.position.set((i - 2.5) * 0.25, 1.3, -0.8)
    scene.add(cube)
    return cube
  })

  renderer.setAnimationLoop(() => {
    cubes.forEach((cube, i) => {
      cube.rotation.y += 0.01 + i * 0.003
      cube.rotation.z += 0.004
    })
    renderer.render(scene, camera)
  })

  window.addEventListener('resize', () => {
    camera.aspect = window.innerWidth / window.innerHeight
    camera.updateProjectionMatrix()
    renderer.setSize(window.innerWidth, window.innerHeight)
  })
}

void main()
