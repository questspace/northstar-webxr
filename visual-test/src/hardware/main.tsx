import { useEffect, useState } from 'react'
import { createRoot } from 'react-dom/client'
import components from '../../../hardware/components.json'
import assumptions from '../../../hardware/design-assumptions.json'
import { PartViewer, referenceParts } from './PartViewer'
import './style.css'
import { ClassCAD } from './ClassCAD'
import { PartStudio } from './PartStudio'
import { JointStudio } from './JointStudio'
import { AssemblyStudio } from './AssemblyStudio'

type Model = {
  source_watts: (load: number, supply: number, resistance: number, efficiency: number) => number
  runtime_hours: (energy: number, usable: number, efficiency: number, source: number) => number
  moment_nm: (mass: number, offset: number) => number
}

function Slider({ label, value, set, min, max, step = 1, unit }: {
  label: string; value: number; set: (n: number) => void; min: number; max: number; step?: number; unit: string
}) {
  return <label className="parameter"><span>{label}<output>{value} {unit}</output></span><input type="range" min={min} max={max} step={step} value={value} onChange={e => set(Number(e.target.value))} /></label>
}

function HardwareWorkbench() {
  const [part, setPart] = useState(referenceParts[0])
  const [selected, setSelected] = useState(components[0])
  const [model, setModel] = useState<Model>()
  const [error, setError] = useState('')
  const [energy, setEnergy] = useState(74)
  const [compute, setCompute] = useState(20)
  const [head, setHead] = useState(8)
  const [voltage, setVoltage] = useState(12)
  const [resistance, setResistance] = useState(0.3)
  const [efficiency, setEfficiency] = useState(90)
  const [frontMass, setFrontMass] = useState(200)
  const [frontOffset, setFrontOffset] = useState(50)
  useEffect(() => {
    fetch('/hardware/model.wasm').then(async response => {
      if (!response.ok) throw new Error('WASM model missing. Run python3 hardware/prepare.py.')
      const result = await WebAssembly.instantiate(await response.arrayBuffer())
      setModel(result.instance.exports as unknown as Model)
    }).catch(e => setError(String(e)))
  }, [])
  const headSource = model?.source_watts(head, voltage, resistance, efficiency / 100) ?? NaN
  const total = headSource + compute
  const runtime = model?.runtime_hours(energy, 0.9, 0.9, total) ?? NaN
  const current = headSource / voltage
  const receiving = voltage - current * resistance
  const moment = model?.moment_nm(frontMass, frontOffset) ?? NaN
  const points = Array.from({ length: 41 }, (_, i) => {
    const hours = i / 10
    const remaining = Math.max(0, energy * 0.9 - hours * total / 0.9)
    return `${40 + i * 10},${145 - remaining / energy * 120}`
  }).join(' ')
  return <main>
    <header><div><p className="eyebrow">Vibestar / Northstar Next</p><h1>Hardware workbench</h1></div><p>Reference inspection · design revision 0<br />Physical fit and power remain unverified</p></header>
    <AssemblyStudio />
    <section className="workspace" aria-label="Reference geometry and parts">
      <div className="reference">
        <div className="section-top"><h2>Reference geometry</h2><label>Part <select value={part} onChange={e => setPart(e.target.value)}>{referenceParts.map(p => <option key={p} value={p}>{p}</option>)}</select></label></div>
        <PartViewer part={part} />
        <p>Original Next candidate, shown in individual print coordinates. This is not the redesigned glasses assembly. Use the STEP to recover assembly placement.</p>
        <a href="/hardware/NSNF_v01-assm.step" download>Download candidate assembly STEP</a>
      </div>
      <aside><h2>Retained hardware</h2><div className="parts" aria-label="Components">{components.map(c => <button key={c.id} aria-pressed={selected.id === c.id} onClick={() => setSelected(c)}>{c.name}<span>×{c.quantity}</span></button>)}</div>
        <h3>{selected.name}</h3><dl><dt>Disposition</dt><dd>{selected.action}</dd><dt>Placement</dt><dd>{selected.location}</dd><dt>Mass</dt><dd>{selected.mass_g === null ? 'Not measured' : `${selected.mass_g} g per bare module`}</dd><dt>Evidence</dt><dd>{selected.evidence}</dd></dl><p>{selected.note}</p>
      </aside>
    </section>
    <ClassCAD />
    <section className="cad"><h2>Starting fit assumptions</h2><p>Next candidate geometry · {assumptions.wearer_ipd_mm} mm wearer IPD target · {assumptions.fit_print.material} fit prints with a {assumptions.fit_print.nozzle_mm} mm nozzle and {assumptions.fit_print.layer_height_mm} mm layers. These are provisional choices for tuning. The IPD target does not reposition the optical reference.</p></section>
    <PartStudio />
    <JointStudio />
    <section className="power" aria-labelledby="power-title">
      <div><p className="eyebrow">Scenario model / Rust WASM</p><h2 id="power-title">Power and endurance</h2><p>Illustrative inputs. Change these to measured values before using the result for component selection.</p>
        <Slider label="Battery energy" value={energy} set={setEnergy} min={20} max={150} unit="Wh" />
        <Slider label="Compute draw at battery output" value={compute} set={setCompute} min={3} max={60} unit="W" />
        <Slider label="Headset load after conversion" value={head} set={setHead} min={1} max={20} unit="W" />
        <Slider label="Cable source voltage" value={voltage} set={setVoltage} min={5} max={20} unit="V" />
        <Slider label="Cable loop resistance" value={resistance} set={setResistance} min={0} max={2} step={0.05} unit="Ω" />
        <Slider label="Headset converter efficiency" value={efficiency} set={setEfficiency} min={70} max={100} unit="%" />
      </div>
      <div className="prediction">
        {error ? <p role="alert">{error}</p> : !model ? <p role="status">Loading Rust model…</p> : <>
          <p className="runtime">{Number.isFinite(runtime) ? runtime.toFixed(2) : 'N/A'} <span>hours estimated</span></p>
          {!Number.isFinite(runtime) ? <p role="alert">No stable operating point in this cable model. Reduce load/resistance or increase source voltage within the actual component ratings.</p> : <>
            <svg viewBox="0 0 470 180" role="img" aria-label={`Estimated usable energy declines to zero in ${runtime.toFixed(2)} hours. Chart spans four hours.`}><path d="M40 20V145H440" fill="none" stroke="currentColor" /><polyline points={points} fill="none" stroke="#235c45" strokeWidth="3" /><text x="34" y="37" textAnchor="end">{(energy * 0.9).toFixed(0)}</text><text x="34" y="149" textAnchor="end">0</text><text x="40" y="170">0 h</text><text x="420" y="170">4 h</text><text x="48" y="20">Usable battery energy (Wh)</text></svg>
            <dl><dt>Battery output draw</dt><dd>{total.toFixed(2)} W</dd><dt>Headset cable current</dt><dd>{current.toFixed(2)} A</dd><dt>Voltage reaching converter</dt><dd>{receiving.toFixed(2)} V</dd><dt>Cable heating</dt><dd>{(current * current * resistance).toFixed(2)} W</dd></dl>
          </>}
          <p>Assumes 90% usable battery energy and 90% bank conversion efficiency. Headset conversion and cable losses are counted separately. Current limits, converter dropout, peak demand and PD negotiation must be checked against selected hardware.</p>
        </>}
      </div>
    </section>
    <section className="balance"><div><h2>Front load at the ear support</h2><p>Illustrative one-axis static moment. This does not predict pressure, comfort or motion stability.</p></div><div><Slider label="Front mass" value={frontMass} set={setFrontMass} min={100} max={400} unit="g" /><Slider label="Forward offset from support" value={frontOffset} set={setFrontOffset} min={10} max={100} unit="mm" /><p>{Number.isFinite(moment) ? `${moment.toFixed(3)} N·m; ${(frontMass * frontOffset / 100).toFixed(0)} g would counterbalance at 100 mm behind that support.` : 'Waiting for model.'}</p></div></section>
    <footer><p>ClassCAD import, joint editing and export run on localhost and in the desktop app. Revision 1 adds removable rail saddles and inward-shaped temples to the reference carrier. The complete wearable frame is still in development.</p><a href="https://github.com/rtenlab/BOXR/tree/76d0f6c09475ad3bf81b5edd614bcd2203d1328f/docs">Reference archive and provenance</a></footer>
  </main>
}

createRoot(document.getElementById('root')!).render(<HardwareWorkbench />)
