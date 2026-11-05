import { useEffect, useState } from 'react'
import type { BufferGeometry } from 'three'
import { openCAD, download } from './cadClient'
import { buildWearablePart, wearableParts } from './wearableRecipe'
import { PartViewer } from './PartViewer'
import { archive } from './archive'
import instructions from '../../../hardware/cad/support-print-instructions.txt?raw'

type Exported = { name: string; content: Uint8Array }

export function AssemblyStudio() {
  const [geometry, setGeometry] = useState<BufferGeometry[]>()
  const [files, setFiles] = useState<Exported[]>([])
  const [selected, setSelected] = useState('')
  const [busy, setBusy] = useState(false)
  const [status, setStatus] = useState('Build the carrier and six removable support parts in their assembly coordinates.')
  const [explode, setExplode] = useState(0)
  useEffect(() => () => geometry?.forEach(g => g.dispose()), [geometry])
  async function build() {
    setBusy(true)
    const meshes: BufferGeometry[] = []
    const exports: Exported[] = []
    let cad: Awaited<ReturnType<typeof openCAD>> | undefined
    try {
      setStatus('Importing the unchanged Next carrier…')
      cad = await openCAD('Wearable reference')
      const response = await fetch('/hardware/NSNF_v01-assm.step')
      if (!response.ok) throw new Error('Reference STEP missing')
      const data = await response.text()
      const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(data))
      const hash = [...new Uint8Array(digest)].map(n => n.toString(16).padStart(2, '0')).join('')
      if (hash !== '003f03d542307b59f0cda3add5112c88f6bb9b16531c2f554d9d41d97df53150')
        throw new Error('Reference STEP differs from the version used to design the rail saddles')
      const loaded = await cad.api.v1.common.load({ data, format: 'STP', doClear: true })
      if (!loaded?.id) throw new Error('Reference import failed')
      const reference = await cad.createBufferGeometry(loaded.id)
      if (reference?.length !== 9) throw new Error('Expected nine pinned reference instances')
      meshes.push(...reference)
      await cad.destroy()
      cad = undefined
      for (const recipe of wearableParts()) {
        setStatus(`Building ${recipe.name}…`)
        cad = await openCAD(recipe.name)
        const id = await buildWearablePart(cad, recipe)
        const result = await cad.createBufferGeometry(id)
        if (result?.length !== 1) throw new Error(`${recipe.name}: expected one solid`)
        result[0].name = recipe.name
        meshes.push(...result)
        for (const format of ['OFB', 'STP', 'STL'] as const) {
          const file = await cad.api.v1.common.save({ format, encoding: 'base64', stl: { binary: true, facetingTol: 0.05, angleTol: 6 } })
          if (!file?.success || !file.content) throw new Error(`${recipe.name}: ${format} export failed`)
          exports.push({ name: `${recipe.name}.${format.toLowerCase()}`, content: Uint8Array.from(atob(file.content), c => c.charCodeAt(0)) })
        }
        await cad.destroy()
        cad = undefined
      }
      exports.push({ name: 'support-recipes.json', content: new TextEncoder().encode(JSON.stringify({
        schema: 'vibestar.support-assembly.v1', units: 'mm', status: 'Fit prototype; not a final wearable',
        reference: 'NSNF_v01-assm.step', source_sha256: hash, parts: wearableParts(),
      }, null, 2)) })
      exports.push({ name: 'READ-ME-FIRST.txt', content: new TextEncoder().encode(instructions) })
      setGeometry(meshes)
      setFiles(exports)
      setSelected(exports[0].name)
      setStatus('Nine unchanged reference instances plus six new support solids. Exports use the same assembly coordinates.')
    } catch (error) {
      meshes.forEach(g => g.dispose())
      setStatus(`Build failed: ${error instanceof Error ? error.message : JSON.stringify(error)}`)
    } finally { await cad?.destroy().catch(() => undefined); setBusy(false) }
  }
  async function save() {
    const file = files.find(f => f.name === selected)
    if (!file) return
    try { await download(file.name, new Uint8Array(file.content)); setStatus(`Saved ${file.name}`) }
    catch (error) { setStatus(String(error)) }
  }
  async function saveAll() {
    try { await download('vibestar-support-assembly.zip', archive(files)); setStatus('Saved support archive') }
    catch (error) { setStatus(String(error)) }
  }
  return <section className="cad" aria-labelledby="support-title" id="support-assembly">
    <p className="eyebrow">Revision 1 / mechanical fit assembly</p>
    <h2 id="support-title">Carrier, hinges and temples</h2>
    <p>Removable saddles grip the candidate’s rear rails. Hinges sit behind the rail ends to leave room for folding. Short rear arms include an open cable channel and strap slots. Grey parts are the unchanged Next reference; amber parts are new.</p>
    <button onClick={build} disabled={busy}>Build support assembly</button>
    <p role="status">{status}</p>
    {geometry && <PartViewer part="Wearable support assembly" assembly={geometry} explode={explode} />}
    <label className="parameter">Separate parts: {explode.toFixed(1)}<input aria-label="Separate support parts" type="range" min="0" max="1" step="0.1" value={explode} onChange={e => setExplode(Number(e.target.value))} /></label>
    <label>Support export <select value={selected} onChange={e => setSelected(e.target.value)} disabled={!files.length || busy}>{files.map(f => <option key={f.name}>{f.name}</option>)}</select></label>{' '}
    <button onClick={save} disabled={!files.length || busy}>Save support file</button>
    <button onClick={saveAll} disabled={!files.length || busy}>Save support archive</button>
    <p>Fit prototype only. Rail dimensions come from NSNF_v01, not the physical headset. Optics, sensor placement, nose support, covers, hinge stops and electronics packaging are not resolved by this assembly.</p>
  </section>
}
