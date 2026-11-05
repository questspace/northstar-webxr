import { useEffect, useState } from 'react'
import type { BufferGeometry } from 'three'
import { openCAD } from './cadClient'
import { buildTemple, defaults } from './frameRecipe'
import { PartViewer } from './PartViewer'

export function JointStudio() {
  const [geometry, setGeometry] = useState<BufferGeometry[]>()
  const [angle, setAngle] = useState(0)
  const [explode, setExplode] = useState(0)
  const [busy, setBusy] = useState(false)
  const [status, setStatus] = useState('Build the default joint to inspect its mating surfaces.')
  useEffect(() => () => geometry?.forEach(g => g.dispose()), [geometry])
  async function build() {
    setBusy(true)
    setStatus('Building arm and fork…')
    let cad: Awaited<ReturnType<typeof openCAD>> | undefined
    const meshes: BufferGeometry[] = []
    try {
      for (const part of ['temple', 'hinge'] as const) {
        cad = await openCAD(`Joint ${part}`)
        const id = await buildTemple(cad, defaults, part)
        const result = await cad.createBufferGeometry(id)
        if (result?.length !== 1) throw new Error('Expected one solid per joint component.')
        meshes.push(...result)
        await cad.destroy()
        cad = undefined
      }
      setGeometry(meshes)
      setStatus('Default 145 mm arm and mating fork. Rotation is a visual inspection, not a collision test.')
    } catch (error) { meshes.forEach(g => g.dispose()); setStatus(error instanceof Error ? error.message : JSON.stringify(error)) }
    finally { await cad?.destroy().catch(() => undefined); setBusy(false) }
  }
  return <section className="cad" aria-labelledby="joint-title">
    <h2 id="joint-title">Joint assembly inspection</h2>
    <p>Default dimensions, independent of the part editor. Align the two bores, slide the arm between the fork tabs, then insert the pivot screw. Screw length and nut retention remain to be selected. Keep the cable outside this joint.</p>
    <button onClick={build} disabled={busy}>Build joint assembly</button>
    <label className="parameter">Fold angle: {angle}°<input aria-label="Fold angle" type="range" min="0" max="90" value={angle} onChange={e => setAngle(Number(e.target.value))} /></label>
    <label className="parameter">Separate components: {explode.toFixed(1)}<input aria-label="Separate joint components" type="range" min="0" max="1" step="0.1" value={explode} onChange={e => { setAngle(0); setExplode(Number(e.target.value)) }} /></label>
    <p role="status">{status}</p>
    <p>Arm: pale grey. Fork: amber. Separate with the arm unfolded to inspect insertion. Ideal CAD clearances are 0.3 mm above and below the arm and at least 2 mm to the backing throughout 0–90° folding. Printing error and deflection are not included.</p>
    {geometry && <PartViewer part="Default joint assembly" assembly={geometry} fold={angle} explode={explode} jointAssembly />}
  </section>
}
