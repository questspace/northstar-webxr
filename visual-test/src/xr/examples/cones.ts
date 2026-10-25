/**
 * three.js `webxr_ar_cones` on NorthStar: pinch to drop a cone where the hand
 * points. Stock example code apart from the runtime boot and a few cubes.
 */
import * as THREE from 'three'
import { ARButton } from 'three/examples/jsm/webxr/ARButton.js'
import { bootNorthStarXR } from './boot'

const main = async () => {
  await bootNorthStarXR()

  const scene = new THREE.Scene()
  const camera = new THREE.PerspectiveCamera(70, window.innerWidth / window.innerHeight, 0.01, 20)
  scene.add(new THREE.HemisphereLight(0xffffff, 0xbbbbff, 3))

  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true })
  renderer.setPixelRatio(1)
  renderer.setSize(window.innerWidth, window.innerHeight)
  renderer.xr.enabled = true
  document.body.appendChild(renderer.domElement)
  document.body.appendChild(ARButton.createButton(renderer, { requiredFeatures: ['hand-tracking'] }))

  const geometry = new THREE.CylinderGeometry(0, 0.05, 0.2, 32).rotateX(Math.PI / 2)
  const onSelect = (controller: THREE.Object3D) => () => {
    const material = new THREE.MeshPhongMaterial({ color: 0xffffff * Math.random() })
    const mesh = new THREE.Mesh(geometry, material)
    mesh.position.set(0, 0, -0.3).applyMatrix4(controller.matrixWorld)
    mesh.quaternion.setFromRotationMatrix(controller.matrixWorld)
    scene.add(mesh)
  }
  for (const i of [0, 1]) {
    const controller = renderer.xr.getController(i)
    controller.addEventListener('select', onSelect(controller))
    scene.add(controller)
  }

  const cubes = Array.from({ length: 8 }, (_, i) => {
    const cube = new THREE.Mesh(
      new THREE.BoxGeometry(0.1, 0.1, 0.1),
      new THREE.MeshPhongMaterial({ color: 0x00ffff, emissive: 0x004444 }),
    )
    cube.position.set(Math.sin(i) * 0.6, 1.3 + (i % 3) * 0.2, -1 - (i % 2) * 0.3)
    scene.add(cube)
    return cube
  })

  renderer.setAnimationLoop(() => {
    cubes.forEach((cube, i) => {
      cube.rotation.y += 0.01 + i * 0.002
      cube.rotation.x += 0.005
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
