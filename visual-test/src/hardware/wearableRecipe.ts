import type { BuerliCadFacade } from '@buerli.io/classcad'

type Vec = [number, number, number]
type Box = { at: Vec; size: Vec; rotation?: Vec }
type Cylinder = { at: Vec; diameter: number; height: number }
type Shape = Box | Cylinder
export type WearablePart = { name: string; add: Shape[]; cut: Shape[] }

// Rail envelopes are from the pinned STEP, not measurements of the user's headset.
export function wearableParts(): WearablePart[] {
  return [-90.03698, 84.67498].flatMap((rail, index) => {
    const side = index === 0 ? -1 : 1
    const name = index === 0 ? 'left' : 'right'
    const pivot = rail + side * 23
    const box = (x: number, y: number, z: number, w: number, d: number, h: number): Box => ({ at: [x, y, z], size: [w, d, h] })
    const bore = (x: number, y: number, z: number, diameter: number, height: number): Cylinder => ({ at: [x, y, z], diameter, height })
    const clampHoles = [rail - 8.5, rail + 8.5].flatMap(x => [89, 111].map(y => bore(x, y, -31, 3.2, 36)))
    const fork = [box(rail - 7.1, 83, -30, 14.2, 34, 30),
      box(Math.min(rail, pivot - 6), 110, -13.3, Math.max(rail, pivot + 6) - Math.min(rail, pivot - 6), 15, 12.6)]
    for (const z of [-13.3, -3.7]) {
      fork.push(box(pivot - 6, 122, z, 12, 10, 3))
    }
    const lower: WearablePart = {
      name: `${name}-rail-saddle`,
      add: [...fork, ...[rail - 8.5, rail + 8.5].flatMap(x => [89, 111].map(y => bore(x, y, -30, 7, 30))), ...[-13.3, -3.7].map(z => bore(pivot, 132, z, 12, 3))],
      cut: [box(rail - 4.1, 82, -27, 8.2, 39, 28), ...clampHoles, bore(pivot, 132, -15, 3.2, 16)],
    }
    const cap: WearablePart = { name: `${name}-rail-cap`, add: [box(rail - 12, 83, 0.6, 24, 34, 3)], cut: clampHoles }
    const ear = side * 75
    const points = [[pivot, 132], [pivot, 146], [ear, 184], [ear, 204]]
    const segments = (width: number, z: number, height: number, start = 0): Shape[] =>
      points.slice(start, -1).flatMap((a, i) => {
        const b = points[start + i + 1]
        const angle = Math.atan2(b[1] - a[1], b[0] - a[0])
        return [{ at: [a[0] + Math.sin(angle) * width / 2, a[1] - Math.cos(angle) * width / 2, z],
          size: [Math.hypot(b[0] - a[0], b[1] - a[1]), width, height], rotation: [0, 0, angle] } as Box,
          bore(a[0], a[1], z, width, height), bore(b[0], b[1], z, width, height)]
      })
    const arm: WearablePart = {
      name: `${name}-rear-temple`,
      add: [...segments(12, -10, 6), box(ear - 6, 192, -24, 12, 12, 15)],
      cut: [bore(pivot, 132, -12, 3.2, 10), ...segments(6, -6, 3, 1),
        box(ear - 3, 194, -25, 6, 6, 7)],
    }
    return [lower, cap, arm]
  })
}

export async function buildWearablePart(cad: BuerliCadFacade, recipe: WearablePart) {
  const api = cad.api.v1.part
  const id = await api.create({ name: recipe.name })
  if (id == null) throw new Error('Could not create part')
  async function shape(shape: Shape, name: string) {
    const datum = await api.workCSys({ id: id!, name: `${name} datum`, offset: shape.at, ...('rotation' in shape ? { rotation: shape.rotation } : {}) })
    if (datum == null) throw new Error(`Could not create ${name} datum`)
    const common = { id: id!, name, references: [datum] }
    const result = 'size' in shape
      ? await api.box({ ...common, length: shape.size[0], width: shape.size[1], height: shape.size[2] })
      : await api.cylinder({ ...common, diameter: shape.diameter, height: shape.height })
    if (result == null) throw new Error(`Could not build ${name}`)
    return result
  }
  const additions = []
  for (const [i, item] of recipe.add.entries()) additions.push(await shape(item, `Body ${i + 1}`))
  const body = additions.length === 1 ? additions[0] : await api.boolean({ id, name: 'Join body', type: 'UNION', target: additions[0], tools: additions.slice(1) })
  if (body == null) throw new Error('Body union failed')
  const cuts = []
  for (const [i, item] of recipe.cut.entries()) cuts.push(await shape(item, `Clearance ${i + 1}`))
  if (await api.boolean({ id, name: 'Cut clearances', type: 'SUBTRACTION', target: body, tools: cuts }) == null)
    throw new Error('Clearance subtraction failed')
  return id
}
