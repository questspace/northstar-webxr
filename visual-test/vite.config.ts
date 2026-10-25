import { defineConfig, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'
import { resolve } from 'node:path'
import { mkdirSync, writeFileSync } from 'node:fs'

/** Dev-only: POST {name, dataUrl} to /__capture saves a panel screenshot under docs/screenshots. */
const capturePlugin = (): Plugin => ({
  name: 'capture',
  configureServer(server) {
    server.middlewares.use('/__capture', (req, res) => {
      let body = ''
      req.on('data', (chunk) => { body += chunk })
      req.on('end', () => {
        const { name, dataUrl } = JSON.parse(body) as { name: string; dataUrl: string }
        const dir = resolve(__dirname, '../docs/screenshots')
        mkdirSync(dir, { recursive: true })
        const file = resolve(dir, `${name.replace(/[^\w-]/g, '_')}.png`)
        writeFileSync(file, Buffer.from(dataUrl.split(',')[1], 'base64'))
        res.end(file)
      })
    })
  },
})

export default defineConfig({
  plugins: [react(), capturePlugin()],
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'index.html'),
        calibration: resolve(__dirname, 'calibration.html'),
        cones: resolve(__dirname, 'examples/cones.html'),
        hands: resolve(__dirname, 'examples/hands.html'),
        menu: resolve(__dirname, 'examples/menu.html'),
        workbench: resolve(__dirname, 'examples/workbench.html'),
      },
    },
  },
  server: {
    port: 5173,
    host: true,
  },
})
