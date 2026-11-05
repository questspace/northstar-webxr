import { defineConfig, loadEnv, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'
import { resolve } from 'node:path'

const localClassCAD = (): Plugin => ({
  name: 'local-classcad-token',
  configureServer(server) {
    server.middlewares.use('/__hardware/classcad-token', (req, res) => {
      const host = req.headers.host
      if (req.method !== 'GET' || host !== 'localhost:5174' ||
          (req.headers.origin && req.headers.origin !== `http://${host}`) ||
          req.headers['sec-fetch-site'] === 'cross-site') {
        res.writeHead(403).end()
        return
      }
      const token = loadEnv('development', resolve(__dirname, '..'), 'CLASSCAD_').CLASSCAD_TOKEN
      res.setHeader('Cache-Control', 'no-store')
      res.setHeader('Content-Type', 'application/json')
      if (!token?.startsWith('ccpk_')) {
        res.writeHead(503).end(JSON.stringify({ error: 'A public CLASSCAD_TOKEN is required in the repository .env.' }))
        return
      }
      res.end(JSON.stringify({ token }))
    })
  },
})

export default defineConfig({
  plugins: [react(), localClassCAD()],
  build: { outDir: 'dist-hardware', rollupOptions: { input: resolve(__dirname, 'hardware.html') } },
  server: { host: 'localhost', port: 5174, strictPort: true },
})
