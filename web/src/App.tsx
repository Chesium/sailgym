import { useEffect, useState } from 'react'

import { loadWasm } from './sim/loadWasm'

type Status =
  | { kind: 'loading' }
  | { kind: 'ready'; version: string }
  | { kind: 'failed'; message: string }

/**
 * Section 01's whole user interface: proof that the WASM module is alive.
 *
 * `data-testid="wasm-status"` and `data-ready` are a contract later sections
 * and every Playwright spec depend on (task 1.3 / 1.4). Do not rename them.
 */
export default function App() {
  const [status, setStatus] = useState<Status>({ kind: 'loading' })

  useEffect(() => {
    let cancelled = false

    loadWasm()
      .then((wasm) => {
        const sim = new wasm.Sim('{}')
        try {
          const version = sim.version()
          if (!cancelled) {
            setStatus({ kind: 'ready', version })
          }
        } finally {
          sim.free()
        }
      })
      .catch((cause: unknown) => {
        if (!cancelled) {
          setStatus({
            kind: 'failed',
            message: cause instanceof Error ? cause.message : String(cause),
          })
        }
      })

    return () => {
      cancelled = true
    }
  }, [])

  if (status.kind === 'ready') {
    return (
      <div data-testid="wasm-status" data-ready="true">
        sailgym {status.version}
      </div>
    )
  }

  if (status.kind === 'failed') {
    return (
      <div data-testid="wasm-status" data-ready="false">
        wasm failed to load: {status.message}
      </div>
    )
  }

  return (
    <div data-testid="wasm-status" data-ready="false">
      loading sailgym…
    </div>
  )
}
