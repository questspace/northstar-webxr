import type { BuerliCadFacade } from '@buerli.io/classcad'
import assumptions from '../../../hardware/design-assumptions.json'

export const defaults = assumptions.temple
export type Temple = typeof defaults
export type PrintablePart = 'temple' | 'hinge'
export const hingeClearance = assumptions.hinge.axial_clearance_each_side_mm
export const hingeTab = assumptions.hinge.tab_thickness_mm
export function partSize(p: Temple, part: PrintablePart) {
  return part === 'temple' ? [p.length, p.width, p.thickness] : [p.width + 9, p.width, p.thickness + 2 * (hingeClearance + hingeTab)]
}
export const fields: { key: keyof Temple; label: string; min: number; max: number; step: number }[] = [
  { key: 'length', label: 'Temple length', min: 110, max: 180, step: 1 },
  { key: 'width', label: 'Temple height', min: 10, max: 20, step: 0.5 },
  { key: 'thickness', label: 'Temple thickness', min: 5, max: 12, step: 0.5 },
  { key: 'screw', label: 'Pivot bore diameter', min: 2, max: 4, step: 0.1 },
  { key: 'channelWidth', label: 'Cable channel width', min: 3, max: 12, step: 0.5 },
  { key: 'channelDepth', label: 'Cable channel depth', min: 1, max: 6, step: 0.5 },
]

export function validateTemple(value: unknown): Temple {
  if (!value || typeof value !== 'object') throw new Error('Expected a temple parameter object.')
  const input = value as Record<string, unknown>
  for (const field of fields) {
    const n = input[field.key]
    if (typeof n !== 'number' || !Number.isFinite(n) || n < field.min || n > field.max)
      throw new Error(`${field.label} must be between ${field.min} and ${field.max} mm.`)
  }
  const p = Object.fromEntries(fields.map(({ key }) => [key, input[key]])) as Temple
  if (p.width - p.channelWidth < 4) throw new Error('Cable channel needs at least 2 mm of material on each side.')
  if (p.thickness - p.channelDepth < 3) throw new Error('Cable channel needs at least 3 mm of material behind it.')
  return p
}

export async function buildTemple(cad: BuerliCadFacade, input: Temple, part: PrintablePart = 'temple') {
  const p = validateTemple(input)
  const api = cad.api.v1.part
  const required = <T,>(value: T | undefined | null, operation: string): T => {
    if (value == null) throw new Error(`ClassCAD failed: ${operation}`)
    return value
  }
  const id = required(await api.create({ name: `${part} fit prototype, mm` }), 'create part')
  async function datum(name: string, offset: [number, number, number]) {
    return required(await api.workCSys({ id, name, offset }), name)
  }
  const r = p.width / 2
  if (part === 'hinge') {
    const bottom = -hingeClearance - hingeTab
    const height = partSize(p, part)[2]
    const cs = await datum('Fork backing origin', [-9, 0, bottom])
    const backing = required(await api.box({ id, name: 'Fork backing', references: [cs], length: 7, width: p.width, height }), 'fork backing')
    const tabs = []
    for (const z of [bottom, p.thickness + hingeClearance]) {
      const tabCS = await datum('Fork tab origin', [-3, 0, z])
      tabs.push(required(await api.box({ id, name: 'Fork tab', references: [tabCS], length: r + 3, width: p.width, height: hingeTab }), 'fork tab'))
      const endCS = await datum('Fork end origin', [r, r, z])
      tabs.push(required(await api.cylinder({ id, name: 'Fork round end', references: [endCS], diameter: p.width, height: hingeTab }), 'fork end'))
    }
    const blank = required(await api.boolean({ id, name: 'Join fork', type: 'UNION', target: backing, tools: tabs }), 'fork union')
    const boreCS = await datum('Fork pivot axis', [r, r, bottom - 1])
    const bore = required(await api.cylinder({ id, name: 'Fork pivot bore', references: [boreCS], diameter: p.screw, height: height + 2 }), 'fork bore')
    required(await api.boolean({ id, name: 'Drill fork', type: 'SUBTRACTION', target: blank, tools: [bore] }), 'fork subtraction')
    return id
  }
  const bodyCS = await datum('Arm origin', [r, 0, 0])
  const body = required(await api.box({ id, name: 'Straight arm', references: [bodyCS], length: p.length - p.width, width: p.width, height: p.thickness }), 'arm')
  const ends = []
  for (const x of [r, p.length - r]) {
    const cs = await datum('Rounded end origin', [x, r, 0])
    ends.push(required(await api.cylinder({ id, name: 'Rounded end', references: [cs], diameter: p.width, height: p.thickness }), 'rounded end'))
  }
  const blank = required(await api.boolean({ id, name: 'Arm blank', type: 'UNION', target: body, tools: ends }), 'arm union')
  const pivotCS = await datum('Pivot bore origin', [r, r, -1])
  const pivot = required(await api.cylinder({ id, name: 'Pivot bore', references: [pivotCS], diameter: p.screw, height: p.thickness + 2 }), 'pivot bore')
  const channelCS = await datum('Open cable channel origin', [p.width + 4, (p.width - p.channelWidth) / 2, p.thickness - p.channelDepth])
  const channel = required(await api.box({ id, name: 'Open cable channel', references: [channelCS], length: p.length, width: p.channelWidth, height: p.channelDepth + 1 }), 'cable channel')
  required(await api.boolean({ id, name: 'Cut bore and cable channel', type: 'SUBTRACTION', target: blank, tools: [pivot, channel] }), 'final subtraction')
  return id
}
