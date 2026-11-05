import type { BuerliCadFacade } from '@buerli.io/classcad'
import { isTauri, invoke } from '@tauri-apps/api/core'

let engine: Promise<typeof import('@buerli.io/classcad')> | undefined

export async function openCAD(name: string): Promise<BuerliCadFacade> {
  engine ??= (async () => {
    let token: string
    if (isTauri() && !import.meta.env.DEV) token = await invoke<string>('public_cad_token')
    else {
      const response = await fetch('/__hardware/classcad-token')
      if (!response.ok) throw new Error('Run hardware:dev on localhost with a public CLASSCAD_TOKEN in .env.')
      token = (await response.json()).token
    }
    const sdk = await import('@buerli.io/classcad')
    sdk.init(id => new sdk.WASMClient(id, { token, logToConsole: false }))
    return sdk
  })().catch(error => { engine = undefined; throw error })
  const { BuerliCadFacade } = await engine
  const cad = new BuerliCadFacade()
  try { await cad.connect(name); return cad }
  catch (error) { await cad.destroy().catch(() => undefined); throw error }
}

export async function download(name: string, content: BlobPart, type = 'application/octet-stream') {
  const blob = new Blob([content], { type })
  if (isTauri()) {
    const { save } = await import('@tauri-apps/plugin-dialog')
    const { writeFile } = await import('@tauri-apps/plugin-fs')
    const path = await save({ defaultPath: name })
    if (!path) throw new Error('Save cancelled; the built model is unchanged.')
    await writeFile(path, new Uint8Array(await blob.arrayBuffer()))
    return
  }
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  link.href = url
  link.download = name
  link.click()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}

export async function exportCAD(cad: BuerliCadFacade, name: string, format: 'OFB' | 'STP' | 'STL') {
  const result = await cad.api.v1.common.save({ format, encoding: 'base64', stl: { binary: true, facetingTol: 0.05, angleTol: 6 } })
  if (!result?.success || !result.content) throw new Error('Engine did not return an export. Check ClassCAD export permissions.')
  await download(`${name}.${format.toLowerCase()}`, Uint8Array.from(atob(result.content), c => c.charCodeAt(0)))
}
