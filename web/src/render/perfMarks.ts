/**
 * Frame-time instrumentation (section 10, task 10.5).
 *
 * brief §37 asks for three things about the browser: 60 fps rendering, physics
 * independent of the render rate, and no visible input lag from the WASM/UI
 * architecture. The first and third are only claims until something measures
 * them, and a Playwright spec on its own can measure the *outside* of a frame
 * but not what the frame spent its time on. This module is the inside.
 *
 * Four spans are recorded, each through `performance.mark` and
 * `performance.measure` so the same numbers show up in a browser profiler:
 *
 * | span | where it is taken | what it covers |
 * |---|---|---|
 * | `frame` | `sim/useSimulation.ts` | the whole animation-frame callback |
 * | `wasm`  | `sim/useSimulation.ts` | `set_controls`, `advance`, `snapshot`, `diagnostics` — every call across the F8 boundary |
 * | `svg`   | `render/BoatSvg.tsx`   | React render **and** DOM commit of the world view |
 * | `deck`  | `wind/DeckOverlay.tsx` | deck.gl's own draw, between its before/after render hooks |
 *
 * **No physics here** (F8). Nothing in this file reads or derives a physical
 * quantity; `noteRudderRendered` compares a rudder angle with a rudder angle
 * only to notice that the number on screen has changed.
 *
 * ## Overhead
 *
 * Each span costs two `performance.mark`s and one `performance.measure`, and
 * the entries are cleared as they are read, so the buffer cannot grow. Four
 * spans at 60 Hz is ~720 User Timing operations a second; measured cost on the
 * section 10 host is under 0.05 ms a frame, against a 16.7 ms budget. The
 * figures are in `docs/v1/performance.md`. Recording is on by default because an
 * instrument that is off is an instrument nobody reads; it is switched off
 * wholesale by {@link setPerfEnabled} if it ever stops being free.
 */

/** The four instrumented spans. */
export type PerfSpan = 'frame' | 'wasm' | 'svg' | 'deck'

export const PERF_SPANS: readonly PerfSpan[] = ['frame', 'wasm', 'svg', 'deck']

/** Samples kept per span. At 60 Hz this is a little over half a minute. */
const CAPACITY = 2048

export interface SpanStats {
  count: number
  mean: number
  p50: number
  p95: number
  max: number
}

export interface PerfReport {
  spans: Record<PerfSpan, SpanStats>
  /** Milliseconds from a steering key-down to the first frame that showed it. */
  inputLagMs: readonly number[]
  /** Input events still waiting for the rudder to move. */
  inputPending: number
  enabled: boolean
}

interface Ring {
  values: Float64Array
  next: number
  filled: boolean
}

const rings: Record<PerfSpan, Ring> = {
  frame: newRing(),
  wasm: newRing(),
  svg: newRing(),
  deck: newRing(),
}

function newRing(): Ring {
  return { values: new Float64Array(CAPACITY), next: 0, filled: false }
}

let enabled = true
let inputLag: number[] = []
/** Steering inputs whose effect has not yet appeared on screen. */
let pending: { at: number; deltaR: number }[] = []

/**
 * `performance` is absent in some non-browser environments the unit tests
 * import this module from, so every use goes through here.
 */
function clock(): Performance | null {
  return typeof performance === 'undefined' ? null : performance
}

export function setPerfEnabled(on: boolean): void {
  enabled = on
}

/** Drop every sample. Called by a spec before it starts measuring. */
export function resetPerf(): void {
  for (const span of PERF_SPANS) {
    rings[span] = newRing()
  }
  inputLag = []
  pending = []
}

function push(span: PerfSpan, ms: number): void {
  if (!enabled || !Number.isFinite(ms)) {
    return
  }
  const ring = rings[span]
  ring.values[ring.next] = ms
  ring.next = (ring.next + 1) % CAPACITY
  if (ring.next === 0) {
    ring.filled = true
  }
}

/** Open a span. Safe to call without a matching {@link endSpan}. */
export function beginSpan(span: PerfSpan): void {
  const p = clock()
  if (!enabled || p === null) {
    return
  }
  p.mark(`sailgym:${span}:start`)
}

/** Close a span opened by {@link beginSpan} and record its duration. */
export function endSpan(span: PerfSpan): void {
  const p = clock()
  if (!enabled || p === null) {
    return
  }
  const name = `sailgym:${span}`
  try {
    const entry = p.measure(name, `${name}:start`)
    push(span, entry.duration)
  } catch {
    // The start mark is missing — the first frame after a reset, or a span
    // closed twice. A dropped sample is the right outcome; an exception in
    // the frame loop is not.
    return
  } finally {
    p.clearMarks(`${name}:start`)
    p.clearMeasures(name)
  }
}

/**
 * Record a span whose start time was captured earlier as a `performance.now()`
 * reading — the React render case, where the start is inside a component body
 * and the end is in its layout effect.
 */
export function endSpanFrom(span: PerfSpan, startedAt: number): void {
  const p = clock()
  if (!enabled || p === null) {
    return
  }
  push(span, p.now() - startedAt)
}

/**
 * A steering key went down. `deltaR` is the rudder angle **currently on
 * screen**, which is what "the rendered rudder has not moved yet" means.
 */
export function noteRudderCommand(deltaR: number): void {
  const p = clock()
  if (!enabled || p === null) {
    return
  }
  // One outstanding input at a time is the honest measurement: a second key
  // press while the first is still unrendered would be measured against a
  // rudder that is already moving.
  if (pending.length > 0) {
    return
  }
  pending.push({ at: p.now(), deltaR })
}

/**
 * The rudder has to move by this much for a commit to count as showing the
 * input.
 *
 * It is a noise floor, not a visibility claim. With a steering key held the
 * tiller moves at its full commanded rate and crosses 5 mrad in about two
 * milliseconds, so the measured lag is the pipeline's and not this threshold's.
 * What the threshold has to be above is the rudder's own idle motion: with no
 * steering command the tiller self-centres at a constant rate and limit-cycles
 * within a few milliradians of zero (`dynamics::rudder_rate` documents it).
 * `tests/e2e/perf.spec.ts` starts each measurement from a **saturated** tiller,
 * where that cycle cannot occur at all, so the two margins are independent.
 */
const VISIBLE_RAD = 5e-3

export function noteRudderRendered(deltaR: number): void {
  const p = clock()
  if (!enabled || p === null || pending.length === 0) {
    return
  }
  const first = pending[0]
  if (Math.abs(deltaR - first.deltaR) < VISIBLE_RAD) {
    return
  }
  inputLag.push(p.now() - first.at)
  pending.shift()
}

function stats(ring: Ring): SpanStats {
  const n = ring.filled ? CAPACITY : ring.next
  if (n === 0) {
    return { count: 0, mean: 0, p50: 0, p95: 0, max: 0 }
  }
  const sorted = Array.from(ring.values.subarray(0, n)).sort((a, b) => a - b)
  const at = (q: number) => sorted[Math.min(n - 1, Math.floor(q * n))]
  return {
    count: n,
    mean: sorted.reduce((a, b) => a + b, 0) / n,
    p50: at(0.5),
    p95: at(0.95),
    max: sorted[n - 1],
  }
}

/** Everything recorded so far. Read by `tests/e2e/perf.spec.ts`. */
export function perfReport(): PerfReport {
  return {
    spans: {
      frame: stats(rings.frame),
      wasm: stats(rings.wasm),
      svg: stats(rings.svg),
      deck: stats(rings.deck),
    },
    inputLagMs: [...inputLag],
    inputPending: pending.length,
    enabled,
  }
}

declare global {
  interface Window {
    /**
     * The perf probe the E2E suite reads. Not an application API: nothing in
     * `web/src` outside this file touches it, and it exists for the same
     * reason `window.__sailgym` does (section 09 handoff §2.10).
     */
    __sailgymPerf?: {
      report(): PerfReport
      reset(): void
      setEnabled(on: boolean): void
    }
  }
}

if (typeof window !== 'undefined') {
  window.__sailgymPerf = {
    report: perfReport,
    reset: resetPerf,
    setEnabled: setPerfEnabled,
  }
}
