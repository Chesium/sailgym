import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'

/**
 * Rust ⇄ TypeScript parity for the debug record (task 8.1, section acceptance
 * criterion 4).
 *
 * Both sides are parsed out of their own declarations — the Rust `struct
 * Diagnostics` block and the TypeScript `interface Diagnostics` block — and
 * compared as **sets, in both directions**. A field added on one side and
 * forgotten on the other fails here, which is the only reason the debug panel
 * can enumerate the TypeScript type and claim to be showing everything the
 * core publishes.
 */

const RUST = '../crates/sailgym-physics/src/diagnostics.rs'
const TS = 'src/sim/diagnostics.ts'

/** The body of a braced block that starts at `opening`, to its closing brace. */
function block(source: string, opening: string): string {
  const start = source.indexOf(opening)
  expect(start, `${opening} must be declared`).toBeGreaterThanOrEqual(0)
  const from = start + opening.length
  const end = source.indexOf('\n}', from)
  expect(end, `${opening} must be closed`).toBeGreaterThan(from)
  return source.slice(from, end)
}

/** `pub name: Type,` declarations, one per field. */
function rustFields(): string[] {
  const body = block(readFileSync(RUST, 'utf8'), 'pub struct Diagnostics {')
  return [...body.matchAll(/^\s*pub (\w+):/gm)].map((m) => m[1])
}

/** `  name: Type` declarations at the interface's own indentation level. */
function tsFields(): string[] {
  const body = block(readFileSync(TS, 'utf8'), 'export interface Diagnostics {')
  return [...body.matchAll(/^ {2}(\w+):/gm)].map((m) => m[1])
}

describe('diagnostics field parity', () => {
  it('parses a non-trivial field list from both sides', () => {
    // Guards the parsers themselves: a regex that silently matched nothing
    // would make every comparison below vacuously true.
    expect(rustFields().length).toBeGreaterThan(40)
    expect(tsFields()).toHaveLength(rustFields().length)
  })

  it('has no field in Rust that is missing from TypeScript', () => {
    const missing = rustFields().filter((f) => !tsFields().includes(f))
    expect(missing).toEqual([])
  })

  it('has no field in TypeScript that is missing from Rust', () => {
    const extra = tsFields().filter((f) => !rustFields().includes(f))
    expect(extra).toEqual([])
  })

  it('keeps the two declarations in the same order', () => {
    // Not required by the acceptance criterion, but a reordered mirror is a
    // review hazard: the two files are read side by side.
    expect(tsFields()).toEqual(rustFields())
  })

  it('covers every brief §30 group by name', () => {
    // The Rust side owns the authoritative §30 audit
    // (`diagnostics::tests::covers_brief_30`). This is the browser-side half:
    // the names the debug panel enumerates really are present here.
    const fields = tsFields()
    for (const required of [
      'true_wind_world',
      'apparent_wind_body',
      'velocity_body',
      'acceleration_body',
      'sail',
      'board',
      'rudder',
      'hull',
      'total_force_h',
      'yaw_moment',
      'heeling_moment',
      'righting_moment',
      'sail_ce_b',
      'board_centre_b',
      'rudder_centre_b',
      'beta_dot',
      'sheet_tension',
      'alpha_sail',
      'alpha_rudder',
      'cl_sail',
      'cd_sail',
      'yaw_rate',
      'roll_rate',
    ]) {
      expect(fields, `brief §30 needs ${required}`).toContain(required)
    }
  })
})
