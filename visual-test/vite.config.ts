import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { resolve } from 'node:path'

export default defineConfig({
  plugins: [react()],
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'index.html'),
        calibration: resolve(__dirname, 'calibration.html'),
      },
    },
  },
  server: {
    port: 5173,
    host: true,
  },
  assetsInclude: ['**/*.glsl'],
})
