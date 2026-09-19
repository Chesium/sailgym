/**
 * The one way the browser reads the wind field.
 *
 * brief section 19 is explicit: **do not perform thousands of individual
 * JS→WASM wind queries per animation frame.** Rust therefore exposes only the
 * batched `sample_wind_grid`; the per-point `sample` is not exported to
 * JavaScript at all. Everything on this side — particle advection, the arrow
 * overlay — reads the cached grid through {@link bilinear}, never the module.
 *
 * There is no wind mathematics here. The grid holds the numbers Rust produced,
 * bit for bit (`crates/sailgym-physics/tests/wind.rs`), and `bilinear` is
 * interpolation between them, not a field model.
 */

/** A rectangle of world, in metres. */
export interface Bounds {
  minX: number
  minY: number
  maxX: number
  maxY: number
}

/**
 * A sampled wind grid. Node `(i, j)` sits at `(x0 + i·dx, y0 + j·dy)` and its
 * `[wx, wy]` pair lives at `data[2·(j·nx + i)]`, row-major — the same layout
 * `WindField::sample_grid` writes.
 */
export interface WindGrid {
  x0: number
  y0: number
  dx: number
  dy: number
  nx: number
  ny: number
  /** `[wx, wy]` pairs, row-major. Length is always `2·nx·ny`. */
  data: Float32Array
}

/** The only part of the `Sim` surface this module needs. */
export interface WindSampler {
  sample_wind_grid(
    x0: number,
    y0: number,
    dx: number,
    dy: number,
    nx: number,
    ny: number,
    t: number,
    out: Float32Array,
  ): void
}

/** Fewest nodes per axis. Two is the minimum a bilinear lookup can work with. */
const MIN_NODES = 2

/**
 * Number of `sample_wind_grid` calls made since the page loaded.
 *
 * `wind.spec.ts` asserts this stays at one per frame. It is a counter rather
 * than a Playwright instrumentation hook because the guarantee — one boundary
 * crossing per frame — is a property of the application, and it should be
 * observable in the application.
 */
let calls = 0

export function gridCallCount(): number {
  return calls
}

/**
 * Return a grid covering `bounds` with roughly `targetCells` nodes.
 *
 * When the node counts are unchanged the **same object and the same
 * `Float32Array`** come back, with the origin and spacing updated in place:
 * this runs every frame, and reallocating a 32 768-element buffer sixty times
 * a second is exactly the kind of garbage the batched interface exists to
 * avoid.
 */
export function ensureGrid(
  cur: WindGrid | null,
  bounds: Bounds,
  targetCells: number,
): WindGrid {
  const width = Math.max(bounds.maxX - bounds.minX, Number.EPSILON)
  const height = Math.max(bounds.maxY - bounds.minY, Number.EPSILON)
  const aspect = width / height

  const ny = Math.max(MIN_NODES, Math.round(Math.sqrt(targetCells / aspect)))
  const nx = Math.max(MIN_NODES, Math.round(targetCells / ny))

  const x0 = bounds.minX
  const y0 = bounds.minY
  const dx = width / (nx - 1)
  const dy = height / (ny - 1)

  if (cur !== null && cur.nx === nx && cur.ny === ny) {
    cur.x0 = x0
    cur.y0 = y0
    cur.dx = dx
    cur.dy = dy
    return cur
  }
  return { x0, y0, dx, dy, nx, ny, data: new Float32Array(2 * nx * ny) }
}

/**
 * Fill `grid.data` from the simulation — **one boundary crossing, whole grid**
 * (brief section 19).
 */
export function sampleInto(sim: WindSampler, grid: WindGrid, t: number): void {
  calls += 1
  sim.sample_wind_grid(
    grid.x0,
    grid.y0,
    grid.dx,
    grid.dy,
    grid.nx,
    grid.ny,
    t,
    grid.data,
  )
}

/**
 * Bilinear lookup at a world point, clamped to the grid's edges.
 *
 * Interpolation, not physics: the wind between two nodes is whatever the
 * visualization needs it to be. Anything that must agree with the simulation
 * asks Rust instead.
 */
export function bilinear(grid: WindGrid, x: number, y: number): [number, number] {
  const fi = clamp((x - grid.x0) / grid.dx, 0, grid.nx - 1)
  const fj = clamp((y - grid.y0) / grid.dy, 0, grid.ny - 1)

  const i0 = Math.min(Math.floor(fi), grid.nx - 2)
  const j0 = Math.min(Math.floor(fj), grid.ny - 2)
  const tx = fi - i0
  const ty = fj - j0

  const a = 2 * (j0 * grid.nx + i0)
  const b = a + 2
  const c = 2 * ((j0 + 1) * grid.nx + i0)
  const d = c + 2

  const w00 = (1 - tx) * (1 - ty)
  const w10 = tx * (1 - ty)
  const w01 = (1 - tx) * ty
  const w11 = tx * ty

  return [
    grid.data[a] * w00 + grid.data[b] * w10 + grid.data[c] * w01 + grid.data[d] * w11,
    grid.data[a + 1] * w00 +
      grid.data[b + 1] * w10 +
      grid.data[c + 1] * w01 +
      grid.data[d + 1] * w11,
  ]
}

function clamp(v: number, lo: number, hi: number): number {
  return v < lo ? lo : v > hi ? hi : v
}
