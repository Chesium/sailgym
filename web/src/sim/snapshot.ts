/**
 * The F8.3 snapshot layout, mirrored on the TypeScript side.
 *
 * `SNAPSHOT_FIELDS` is the single shared constant list: the Rust source of
 * truth is `sailgym_physics::state::STATE_FIELDS`, and the Rust test
 * `simulation::snapshot_layout` reads this file and asserts the two agree in
 * length and order. Reordering one without the other fails the gate.
 *
 * No physics lives here (F8) — this is an index map and nothing more.
 */

export const SNAPSHOT_FIELDS = [
  'x',
  'y',
  'psi',
  'phi',
  'u',
  'v',
  'r',
  'p',
  'beta',
  'betaDot',
  'deltaR',
  'lSheet',
  't',
] as const

export type SnapshotField = (typeof SNAPSHOT_FIELDS)[number]
export type Snapshot = Record<SnapshotField, number>

/** Number of scalars in the flat buffer `Sim.snapshot()` returns. */
export const SNAPSHOT_LENGTH = SNAPSHOT_FIELDS.length

/**
 * Read a flat `Sim.snapshot()` buffer into a named record.
 *
 * Throws on a wrong-length buffer rather than silently producing `undefined`
 * fields: a length mismatch means the WASM build and this file disagree, which
 * is exactly the drift the shared list exists to prevent.
 */
export function readSnapshot(buf: Float64Array): Snapshot {
  if (buf.length !== SNAPSHOT_LENGTH) {
    throw new Error(`snapshot: expected ${SNAPSHOT_LENGTH} values, got ${buf.length}`)
  }
  const out = {} as Snapshot
  for (let i = 0; i < SNAPSHOT_FIELDS.length; i += 1) {
    out[SNAPSHOT_FIELDS[i]] = buf[i]
  }
  return out
}
