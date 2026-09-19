import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import App from './App'

const container = document.getElementById('root')

// `index.html` always ships #root; this is a type narrowing, not a runtime path
// we expect to take. Rendering nothing is deterministic and silent.
if (container !== null) {
  createRoot(container).render(
    <StrictMode>
      <App />
    </StrictMode>,
  )
}
