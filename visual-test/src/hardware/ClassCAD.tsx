import { useEffect, useRef, useState } from 'react'
import type { BuerliCadFacade } from '@buerli.io/classcad'
import type { BufferGeometry } from 'three'
import { PartViewer } from './PartViewer'
import { openCAD, exportCAD, download } from './cadClient'

export function ClassCAD() {
  const facade = useRef<BuerliCadFacade>()
  const [geometry, setGeometry] = useState<BufferGeometry[]>()
  const [status, setStatus] = useState('Connect to import the upstream STEP with its assembly placements.')
  const [busy, setBusy] = useState(false)
  const [explode, setExplode] = useState(0)
  const [sourceHash, setSourceHash] = useState('')
  useEffect(() => () => { void facade.current?.destroy().catch(() => undefined) }, [])
  useEffect(() => () => geometry?.forEach(g => g.dispose()), [geometry])

  async function connect() {
    setBusy(true)
    setStatus('Starting ClassCAD and importing the reference STEP…')
    let cad: BuerliCadFacade | undefined
    try {
      cad = await openCAD('Northstar Next reference')
      const step = await fetch('/hardware/NSNF_v01-assm.step')
      if (!step.ok) throw new Error('Reference STEP missing. Run hardware/prepare.py.')
      const data = await step.text()
      const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(data))
      const hash = [...new Uint8Array(digest)].map(n => n.toString(16).padStart(2, '0')).join('')
      const model = await cad.api.v1.common.load({ data, format: 'STP', doClear: true })
      if (!model?.id) throw new Error('ClassCAD did not return an imported model.')
      const meshes = await cad.createBufferGeometry(model.id)
      if (!meshes?.length) throw new Error('ClassCAD imported no displayable geometry.')
      await facade.current?.destroy().catch(() => undefined)
      facade.current = cad
      cad = undefined
      setGeometry(meshes)
      setSourceHash(hash)
      setStatus(`Imported ${meshes.length} placed part instances. Reference geometry only; physical match remains unverified.`)
    } catch (error) {
      setStatus(`ClassCAD could not import: ${error instanceof Error ? error.message : String(error)}`)
    } finally { await cad?.destroy().catch(() => undefined); setBusy(false) }
  }

  async function saveMeasurements() {
    if (!geometry) return
    setBusy(true)
    try {
      const instances = geometry.map((mesh, index) => {
        mesh.computeBoundingBox()
        const bounds = mesh.boundingBox!
        return { index, name: mesh.name || null, bounds: [bounds.min.toArray(), bounds.max.toArray()],
          vertex_count: mesh.getAttribute('position').count,
          coordinates: 'Assembly placement baked into mesh; not a component-local transform' }
      })
      await download('next-assembly-measurements.json', JSON.stringify({
        schema: 'vibestar.reference-measurements.v1', source_sha256: sourceHash,
        coordinate_units: 'ClassCAD engine units; compare against a known physical length before use',
        status: 'Mechanical reference only. Does not contain calibrated optical surfaces or eye poses.',
        instances,
      }, null, 2), 'application/json')
      setStatus('Assembly bounds exported. View separation is excluded from measurements.')
    } catch (error) { setStatus(String(error)) }
    finally { setBusy(false) }
  }

  async function save(format: 'OFB' | 'STP') {
    setBusy(true)
    try {
      await exportCAD(facade.current!, 'northstar-next-reference', format)
      setStatus(`${format} exported. View separation does not change CAD placements.`)
    } catch (error) {
      setStatus(`Export failed: ${error instanceof Error ? error.message : String(error)}`)
    } finally { setBusy(false) }
  }

  return <section className="cad" aria-labelledby="cad-title">
    <h2 id="cad-title">ClassCAD assembly</h2>
    <button onClick={connect} disabled={busy || !!geometry}>Import Next STEP</button>{' '}
    <button onClick={() => save('OFB')} disabled={busy || !geometry}>Export native OFB</button>{' '}
    <button onClick={() => save('STP')} disabled={busy || !geometry}>Export STEP</button>
    <button onClick={saveMeasurements} disabled={busy || !geometry}>Export assembly measurements</button>
    <p role="status">{status}</p>
    {geometry && <><label>View separation <input type="range" min="0" max="2" step="0.1" value={explode} onChange={e => setExplode(Number(e.target.value))} /></label><PartViewer part="Next assembly" assembly={geometry} explode={explode} /><p>Radial separation for inspection; this is not an assembly sequence. Optical parts stay fixed in the CAD model.</p></>}
  </section>
}
