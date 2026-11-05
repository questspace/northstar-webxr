import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { loadEnv } from 'vite'

const root = fileURLToPath(new URL('../', import.meta.url))
const token = process.env.CLASSCAD_TOKEN ?? loadEnv('development', root, 'CLASSCAD_').CLASSCAD_TOKEN
if (!token?.startsWith('ccpk_')) throw new Error('Set a public CLASSCAD_TOKEN in the repository .env.')
const executable = process.platform === 'darwin'
  ? 'src-tauri/target/release/bundle/macos/Vibestar Hardware.app/Contents/MacOS/vibestar-hardware'
  : `src-tauri/target/release/vibestar-hardware${process.platform === 'win32' ? '.exe' : ''}`
const child = spawn(fileURLToPath(new URL(executable, import.meta.url)), [], {
  env: { ...process.env, CLASSCAD_TOKEN: token }, stdio: 'inherit',
})
child.on('error', error => { console.error(`Build with npm run hardware:desktop:build first. ${error.message}`); process.exitCode = 1 })
child.on('exit', code => { process.exitCode = code ?? 1 })
