import { useEffect, useRef, useState } from 'react'
import * as THREE from 'three/webgpu'
import { STLLoader } from 'three/addons/loaders/STLLoader.js'
import { OrbitControls } from 'three/addons/controls/OrbitControls.js'

export const referenceParts = ['mainbracket', 'electronicsmount', 'leftdisplaymount', 'leftslide', 'ribbonmanager', 'cableclamp', 'XR50 + SIR170 mount']

export function PartViewer({ part, assembly, explode = 0, fold = 0, pivot = 6, jointAssembly = false }: { part: string; assembly?: THREE.BufferGeometry[]; explode?: number; fold?: number; pivot?: number; jointAssembly?: boolean }) {
  const host = useRef<HTMLDivElement>(null)
  const separation = useRef(explode)
  separation.current = explode
  const joint = useRef({ fold, pivot })
  joint.current = { fold, pivot }
  const [status, setStatus] = useState('Loading reference geometry…')
  useEffect(() => {
    const container = host.current!
    let cancelled = false
    let cleanup = () => {}
    setStatus('Loading reference geometry…')
    async function mount() {
      const renderer = new THREE.WebGPURenderer({ antialias: true })
      const scene = new THREE.Scene()
      scene.background = new THREE.Color('#172126')
      const camera = new THREE.PerspectiveCamera(40, 1, 0.01, 10000)
      const controls = new OrbitControls(camera, renderer.domElement)
      let geometries: THREE.BufferGeometry[] = []
      const materials = ['#d3ddd8', '#dba956'].map(color => new THREE.MeshStandardMaterial({ color, roughness: 0.65, metalness: 0.05 }))
      const observer = new ResizeObserver(() => {
        const { width, height } = container.getBoundingClientRect()
        if (width <= 0 || height <= 0) return
        camera.aspect = width / Math.max(height, 1)
        camera.updateProjectionMatrix()
        renderer.setSize(width, height)
      })
      let disposed = false
      cleanup = () => {
        if (disposed) return
        disposed = true
        observer.disconnect()
        renderer.setAnimationLoop(null)
        controls.dispose()
        geometries.forEach(g => g.dispose())
        materials.forEach(material => material.dispose())
        renderer.dispose()
        renderer.domElement.remove()
      }
      try {
        await renderer.init()
        if (cancelled) { cleanup(); return }
        const source = part === 'XR50 + SIR170 mount' ? '/hardware/NorthStar-Next-XR50-IR170-Mount.stl' : `/hardware/NSNF_v01-${part}.stl`.replace('v01-mainbracket', 'v01_mainbracket')
        geometries = assembly ? assembly.map(g => g.clone()) : [await new STLLoader().loadAsync(source)]
        if (cancelled) { geometries.forEach(g => g.dispose()); cleanup(); return }
        const bounds = new THREE.Box3()
        for (const geometry of geometries) {
          geometry.computeBoundingBox()
          bounds.union(geometry.boundingBox!)
        }
        const size = bounds.getSize(new THREE.Vector3())
        const center = jointAssembly ? new THREE.Vector3(pivot, pivot, 0) : bounds.getCenter(new THREE.Vector3())
        const radius = Math.max(size.x, size.y, size.z)
        const instances: { mesh: THREE.Mesh; offset: THREE.Vector3 }[] = []
        for (const [index, geometry] of geometries.entries()) {
          const offset = jointAssembly ? new THREE.Vector3(index === 0 ? 40 : -40, 0, 0) : geometry.boundingBox!.getCenter(new THREE.Vector3()).sub(center)
          const mesh = new THREE.Mesh(geometry, materials[jointAssembly ? index % 2 : geometry.name ? 1 : 0])
          mesh.position.copy(center).negate().addScaledVector(offset, explode)
          scene.add(mesh)
          instances.push({ mesh, offset })
        }
        scene.add(new THREE.HemisphereLight(0xffffff, 0x5a6b75, 3))
        const key = new THREE.DirectionalLight(0xffffff, 4)
        key.position.set(1, 2, 3)
        scene.add(key)
        camera.position.set(radius * 0.8, -radius * 1.1, radius * 1.6)
        camera.up.set(0, 0, 1)
        controls.update()
        renderer.setPixelRatio(Math.min(devicePixelRatio, 2))
        container.append(renderer.domElement)
        observer.observe(container)
        setStatus(`${'isWebGPUBackend' in renderer.backend && renderer.backend.isWebGPUBackend ? 'WebGPU' : 'WebGL fallback'} · drag to orbit · scroll to zoom`)
        renderer.setAnimationLoop(() => {
          if (!container.isConnected || container.clientWidth === 0 || container.clientHeight === 0) return
          instances.forEach(({ mesh, offset }, index) => {
            mesh.position.copy(center).negate().addScaledVector(offset, separation.current)
            const angle = jointAssembly && index === 0 ? joint.current.fold * Math.PI / 180 : 0
            mesh.rotation.z = angle
            const p = joint.current.pivot
            mesh.position.x += p - p * Math.cos(angle) + p * Math.sin(angle)
            mesh.position.y += p - p * Math.sin(angle) - p * Math.cos(angle)
          })
          controls.update()
          renderer.render(scene, camera)
        })
      } catch (error) {
        if (!cancelled) setStatus(`Viewer unavailable: ${error instanceof Error ? error.message : String(error)}. Run python3 hardware/prepare.py if assets are missing.`)
        cleanup()
      }
    }
    void mount()
    return () => { cancelled = true; cleanup() }
  }, [part, assembly, jointAssembly, pivot])
  return <><div ref={host} className="part-viewport" aria-label={`3D reference part: ${part}`} /><p role="status" className="viewer-status">{status}</p></>
}
