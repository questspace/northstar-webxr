import { useEffect, useRef, useState } from 'react'
import type { BuerliCadFacade } from '@buerli.io/classcad'
import { Box3, Vector3, type BufferGeometry } from 'three'
import { PartViewer } from './PartViewer'
import { download, exportCAD, openCAD } from './cadClient'
import { buildTemple, defaults, fields, partSize, validateTemple, type Temple, type PrintablePart } from './frameRecipe'

export function PartStudio() {
  const cad = useRef<BuerliCadFacade>()
  const [parameters, setParameters] = useState<Temple>(defaults)
  const [built, setBuilt] = useState<Temple>()
  const [part, setPart] = useState<PrintablePart>('temple')
  const [builtPart, setBuiltPart] = useState<PrintablePart>()
  const [geometry, setGeometry] = useState<BufferGeometry[]>()
  const [busy, setBusy] = useState(false)
  const [status, setStatus] = useState('Set dimensions and generate a separate temple fit prototype.')
  const dirty = part !== builtPart || JSON.stringify(parameters) !== JSON.stringify(built)
  useEffect(() => () => { void cad.current?.destroy().catch(() => undefined) }, [])
  useEffect(() => () => geometry?.forEach(g => g.dispose()), [geometry])

  async function generate() {
    setBusy(true)
    setStatus('Building native ClassCAD features…')
    let next: BuerliCadFacade | undefined
    try {
      const p = validateTemple(parameters)
      next = await openCAD('Vibestar temple prototype')
      const id = await buildTemple(next, p, part)
      const meshes = await next.createBufferGeometry(id)
      if (!meshes?.length) throw new Error('No solid geometry returned.')
      const bounds = new Box3()
      meshes.forEach(g => { g.computeBoundingBox(); bounds.union(g.boundingBox!) })
      const size = bounds.getSize(new Vector3())
      const expected = partSize(p, part)
      if (size.toArray().some((n, i) => Math.abs(n - expected[i]) > 0.1)) {
        meshes.forEach(g => g.dispose())
        throw new Error(`Unexpected engine dimensions: ${size.toArray().map(n => n.toFixed(2)).join(' × ')}. Export withheld.`)
      }
      await cad.current?.destroy().catch(() => undefined)
      cad.current = next
      next = undefined
      setGeometry(meshes)
      setBuilt({ ...p })
      setBuiltPart(part)
      setStatus(`Built ${size.toArray().map(n => n.toFixed(1)).join(' × ')} mm. Prototype only: hinge attachment and wearer fit are not validated.`)
    } catch (error) { setStatus(String(error)) }
    finally { await next?.destroy().catch(() => undefined); setBusy(false) }
  }

  async function save(format: 'OFB' | 'STP' | 'STL') {
    if (!cad.current || dirty) return
    setBusy(true)
    try {
      await exportCAD(cad.current, `vibestar-${part}-prototype-mm`, format)
      setStatus(`${format} exported from the built solid. Units: mm. Requires fit and strength testing.`)
    } catch (error) { setStatus(String(error)) }
    finally { setBusy(false) }
  }

  async function load(file?: File) {
    if (!file) return
    try {
      const recipe = JSON.parse(await file.text())
      if (!['vibestar.temple.v1', 'vibestar.part.v2'].includes(recipe.schema) || recipe.units !== 'mm') throw new Error('Unsupported recipe or units.')
      if (recipe.schema === 'vibestar.part.v2' && !['temple', 'hinge'].includes(recipe.part)) throw new Error('Unsupported part.')
      setParameters(validateTemple(recipe.parameters))
      if (recipe.schema === 'vibestar.part.v2') setPart(recipe.part)
      setStatus('Recipe loaded. Generate to rebuild the solid.')
    } catch (error) { setStatus(String(error)) }
  }

  return <section className="cad" aria-labelledby="studio-title">
    <h2 id="studio-title">Temple part editor</h2>
    <p>Generate the straight cable-channel arm or its mating fork for a joint fit test. The fork has 3 mm tabs and 0.3 mm clearance per side. Frame attachment, folding stops and ear contour remain unvalidated. Edits never alter the optical reference assembly.</p>
    <label>Print part <select value={part} disabled={busy} onChange={e => setPart(e.target.value as PrintablePart)}><option value="temple">Temple arm</option><option value="hinge">Mating hinge fork</option></select></label>
    <div className="studio-fields">{fields.map(field => <label key={field.key}>{field.label} (mm)<input type="number" {...{ min: field.min, max: field.max, step: field.step }} value={parameters[field.key]} disabled={busy} onChange={e => setParameters(p => ({ ...p, [field.key]: e.target.valueAsNumber }))} /></label>)}</div>
    <button disabled={busy} onClick={generate}>Generate {part}</button>
    {(['OFB', 'STP', 'STL'] as const).map(format => <button key={format} disabled={busy || !geometry || dirty} onClick={() => save(format)}>Export {part} {format}</button>)}
    <button disabled={busy} onClick={async () => {
      try { await download(`${part}-recipe.json`, JSON.stringify({ schema: 'vibestar.part.v2', units: 'mm', part, parameters: validateTemple(parameters) }, null, 2), 'application/json') }
      catch (error) { setStatus(String(error)) }
    }}>Save dimensions</button>
    <label className="file-input">Load dimensions <input type="file" accept=".json" disabled={busy} onChange={e => { void load(e.target.files?.[0]); e.target.value = '' }} /></label>
    <p role="status">{status}{geometry && dirty ? ' Dimensions changed; regenerate before exporting.' : ''}</p>
    {geometry && <PartViewer part={`${builtPart} fit prototype`} assembly={geometry} />}
  </section>
}
