/**
 * Vibestar Server
 *
 * Single-command launch for the NorthStar AR experience.
 * - Spawns xvisio_test as child process for 6DOF tracking
 * - WebSocket bridge for XR50 pose data
 * - Serves the Vite production build (or proxies to Vite dev server)
 *
 * Usage:
 *   Development:  npm run dev    (in another terminal) + node server.js
 *   Production:   sudo npm start (spawns xvisio_test and serves built files)
 */

import { WebSocketServer } from 'ws'
import { createInterface } from 'readline'
import { createServer } from 'http'
import { readFileSync, existsSync } from 'fs'
import { spawn } from 'child_process'
import { join, extname, resolve } from 'path'
import { fileURLToPath } from 'url'

const __dirname = fileURLToPath(new URL('.', import.meta.url))

const PORT = parseInt(process.env.PORT ?? '8080', 10)
const XVISIO_BIN = resolve(
  __dirname,
  process.platform === 'win32'
    ? '../xvisio-rs/target/release/examples/stream_json.exe'
    : '../xvisio-rs/target/release/examples/stream_json'
)
const DIST_DIR = resolve(__dirname, 'dist')
const AUTO_SPAWN = process.env.NO_SPAWN !== '1' && existsSync(XVISIO_BIN)

const clients = new Set()

// ---- MIME types ----
const MIME = {
  '.html': 'text/html',
  '.css': 'text/css',
  '.js': 'application/javascript',
  '.mjs': 'application/javascript',
  '.json': 'application/json',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.svg': 'image/svg+xml',
  '.woff2': 'font/woff2',
  '.glsl': 'text/plain',
}

// ---- HTTP server: serves built files from dist/ ----
const http = createServer((req, res) => {
  let urlPath = req.url?.split('?')[0] ?? '/'
  if (urlPath === '/') urlPath = '/index.html'

  // Try dist/ first, then public/
  const distPath = join(DIST_DIR, urlPath)
  const publicPath = join(__dirname, 'public', urlPath)

  let content
  let filePath
  try {
    content = readFileSync(distPath)
    filePath = distPath
  } catch {
    try {
      content = readFileSync(publicPath)
      filePath = publicPath
    } catch {
      // SPA fallback: serve index.html for any unmatched route
      try {
        content = readFileSync(join(DIST_DIR, 'index.html'))
        filePath = join(DIST_DIR, 'index.html')
      } catch {
        res.writeHead(404)
        res.end('Not found. Run `npm run build` first.')
        return
      }
    }
  }

  const ext = extname(filePath)
  const mime = MIME[ext] || 'application/octet-stream'
  res.writeHead(200, {
    'Content-Type': mime,
    'Cache-Control': ext === '.html' ? 'no-cache' : 'max-age=31536000',
    'Access-Control-Allow-Origin': '*',
  })
  res.end(content)
})

// ---- WebSocket server ----
const wss = new WebSocketServer({ server: http })

wss.on('connection', (ws) => {
  clients.add(ws)
  console.log(`[WS] Client connected (${clients.size} total)`)
  ws.on('close', () => {
    clients.delete(ws)
    console.log(`[WS] Client disconnected (${clients.size} total)`)
  })
})

const broadcast = (data) => {
  for (const ws of clients) {
    if (ws.readyState === 1) ws.send(data)
  }
}

// ---- XR50 data source ----
// If piped from stdin: reads from stdin
// If auto-spawn: spawns xvisio_test child process
if (!process.stdin.isTTY) {
  // Data piped from stdin (legacy mode: sudo xvisio_test | node server.js)
  console.log('[XR50] Reading pose data from stdin pipe...')
  const rl = createInterface({ input: process.stdin })
  rl.on('line', broadcast)
} else if (AUTO_SPAWN) {
  console.log(`[XR50] Spawning: ${XVISIO_BIN}`)
  const child = spawn(XVISIO_BIN, [], { stdio: ['ignore', 'pipe', 'inherit'] })

  const rl = createInterface({ input: child.stdout })
  rl.on('line', broadcast)

  child.on('error', (err) => console.error('[XR50] Spawn error:', err.message))
  child.on('exit', (code) => console.log(`[XR50] Process exited (code ${code})`))

  // Clean up on server shutdown
  const cleanup = () => {
    child.kill('SIGTERM')
    process.exit(0)
  }
  process.on('SIGINT', cleanup)
  process.on('SIGTERM', cleanup)
} else {
  console.log('[XR50] No stdin pipe and xvisio_test not found. Waiting for data...')
  console.log(`       Expected binary at: ${XVISIO_BIN}`)
  console.log('       Or pipe data: sudo ../libxvisio/build/xvisio_test | node server.js')
}

// ---- Start ----
http.listen(PORT, () => {
  console.log('')
  console.log('  ╔══════════════════════════════════════╗')
  console.log('  ║          VIBESTAR  SERVER             ║')
  console.log('  ╠══════════════════════════════════════╣')
  console.log(`  ║  http://localhost:${PORT}              ║`)
  console.log(`  ║  Desktop:  http://localhost:${PORT}    ║`)
  console.log(`  ║  Headset:  http://localhost:${PORT}/#headset  ║`)
  console.log('  ╚══════════════════════════════════════╝')
  console.log('')
})
