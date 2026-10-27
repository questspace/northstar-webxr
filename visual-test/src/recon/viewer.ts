/**
 * Viewer for the reconstruction's PLY mesh (`xvisio-rs --example reconstruct`), in the XR50's
 * SLAM world converted to Three.js (x, -y, -z). `?file=/recon/other.ply` picks another file.
 */
import * as THREE from 'three'
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'
import { PLYLoader } from 'three/examples/jsm/loaders/PLYLoader.js'

const file = new URLSearchParams(location.search).get('file') ?? '/recon/room.ply'
const info = document.getElementById('info')!

const renderer = new THREE.WebGLRenderer({ antialias: true })
renderer.setPixelRatio(window.devicePixelRatio)
renderer.setSize(window.innerWidth, window.innerHeight)
document.body.appendChild(renderer.domElement)

const scene = new THREE.Scene()
scene.add(new THREE.HemisphereLight(0xdde8ff, 0x223344, 1.6))
const sun = new THREE.DirectionalLight(0xffffff, 1.2)
sun.position.set(1, 3, 2)
scene.add(sun)
scene.add(new THREE.AxesHelper(0.3))

const camera = new THREE.PerspectiveCamera(60, window.innerWidth / window.innerHeight, 0.01, 50)
camera.position.set(0.8, 0.8, 1.2)
const controls = new OrbitControls(camera, renderer.domElement)

new PLYLoader().load(file, (geometry) => {
  // XR50 world (y down, z forward) → Three.js (y up, z back).
  geometry.applyMatrix4(new THREE.Matrix4().makeScale(1, -1, -1))
  geometry.computeBoundingBox()
  const box = geometry.boundingBox!
  const mesh = new THREE.Mesh(geometry, new THREE.MeshStandardMaterial({ color: 0xb8c4d0, roughness: 0.85, side: THREE.DoubleSide, flatShading: false }))
  scene.add(mesh)
  scene.add(new THREE.GridHelper(4, 40, 0x335, 0x223).translateY(box.min.y))
  const center = box.getCenter(new THREE.Vector3())
  controls.target.copy(center)
  camera.position.copy(center).add(new THREE.Vector3(1.2, 1.0, 1.6))
  controls.update()
  const size = box.getSize(new THREE.Vector3())
  info.textContent = `${file}: ${(geometry.index ? geometry.index.count / 3 : 0).toLocaleString()} triangles, ${size.x.toFixed(2)} × ${size.y.toFixed(2)} × ${size.z.toFixed(2)} m (drag to orbit)`
}, undefined, () => { info.textContent = `Could not load ${file}` })

renderer.setAnimationLoop(() => renderer.render(scene, camera))
window.addEventListener('resize', () => {
  camera.aspect = window.innerWidth / window.innerHeight
  camera.updateProjectionMatrix()
  renderer.setSize(window.innerWidth, window.innerHeight)
})
