export function archive(files: { name: string; content: Uint8Array }[]) {
  const encoder = new TextEncoder()
  const chunks: Uint8Array<ArrayBuffer>[] = []
  const directory: Uint8Array<ArrayBuffer>[] = []
  let offset = 0
  for (const file of files) {
    const name = encoder.encode(file.name)
    let crc = 0xffffffff
    for (const byte of file.content) {
      crc ^= byte
      for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0)
    }
    crc = (crc ^ 0xffffffff) >>> 0
    const local = new Uint8Array(30 + name.length)
    const view = new DataView(local.buffer)
    view.setUint32(0, 0x04034b50, true)
    view.setUint16(4, 20, true)
    view.setUint16(6, 0x800, true)
    view.setUint16(12, 33, true)
    view.setUint32(14, crc, true)
    view.setUint32(18, file.content.length, true)
    view.setUint32(22, file.content.length, true)
    view.setUint16(26, name.length, true)
    local.set(name, 30)
    const central = new Uint8Array(46 + name.length)
    const entry = new DataView(central.buffer)
    entry.setUint32(0, 0x02014b50, true)
    entry.setUint16(4, 20, true)
    central.set(local.subarray(4, 30), 6)
    entry.setUint32(42, offset, true)
    central.set(name, 46)
    chunks.push(local, new Uint8Array(file.content))
    directory.push(central)
    offset += local.length + file.content.length
  }
  const end = new Uint8Array(22)
  const footer = new DataView(end.buffer)
  footer.setUint32(0, 0x06054b50, true)
  footer.setUint16(8, files.length, true)
  footer.setUint16(10, files.length, true)
  footer.setUint32(12, directory.reduce((n, entry) => n + entry.length, 0), true)
  footer.setUint32(16, offset, true)
  return new Blob([...chunks, ...directory, end], { type: 'application/zip' })
}
