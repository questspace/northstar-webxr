import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { CalibrationApp } from './CalibrationApp'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <CalibrationApp />
  </StrictMode>,
)
