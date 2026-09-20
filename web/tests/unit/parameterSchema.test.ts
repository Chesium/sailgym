import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'

import {
  BRIEF_31_PARAMETERS,
  buildSchema,
  groupSchema,
  leafPaths,
  PARAM_TAGS,
  stepFor,
  type ParamMeta,
} from '../../src/ui/parameterSchema'
import { fullTravel } from '../../src/ui/TouchControls'
import { ilcaParams } from './ilca'

/**
 * A sample `parameters_json()` document: one group of each shape the real
 * catalogue has — plain scalars, a flattened foil section, a vector
 * parameter, a flag, and the non-scalar `sim.integrator`.
 */
const VALUES = {
  hull: { loa: 4.23, m_hull: 58, sailor_pos_b: { x: 0, y: 0, z: 0.35 } },
  sail: { area: 7.06, cd0: 0.06, mast_pos_b: { x: 1.2, y: 0, z: 0 } },
  rudder: { delta_r_max: 0.698, delta_r_self_centre: true },
  sim: { dt: 0.005, integrator: 'Rk2Midpoint' },
}

const META: ParamMeta[] = [
  { path: 'hull.loa', tag: 'KNOWN', unit: 'm', doc: 'length overall', kind: 'f64', reset_required: false },
  { path: 'hull.m_hull', tag: 'KNOWN', unit: 'kg', doc: 'hull mass', kind: 'f64', reset_required: false },
  { path: 'hull.sailor_pos_b.x', tag: 'ASSUMED', unit: 'm, in B', doc: '', kind: 'f64', reset_required: false },
  { path: 'hull.sailor_pos_b.y', tag: 'ASSUMED', unit: 'm, in B', doc: '', kind: 'f64', reset_required: false },
  { path: 'hull.sailor_pos_b.z', tag: 'ASSUMED', unit: 'm, in B', doc: '', kind: 'f64', reset_required: false },
  { path: 'sail.area', tag: 'KNOWN', unit: 'm²', doc: 'sail area', kind: 'f64', reset_required: false },
  { path: 'sail.cd0', tag: 'ASSUMED', unit: 'dimensionless', doc: '', kind: 'f64', reset_required: false },
  { path: 'sail.mast_pos_b.x', tag: 'ASSUMED', unit: 'm, in B', doc: '', kind: 'f64', reset_required: false },
  { path: 'sail.mast_pos_b.y', tag: 'ASSUMED', unit: 'm, in B', doc: '', kind: 'f64', reset_required: false },
  { path: 'sail.mast_pos_b.z', tag: 'ASSUMED', unit: 'm, in B', doc: '', kind: 'f64', reset_required: false },
  { path: 'rudder.delta_r_max', tag: 'ASSUMED', unit: 'rad', doc: '', kind: 'f64', reset_required: false },
  {
    path: 'rudder.delta_r_self_centre',
    tag: 'TUNABLE',
    unit: '',
    doc: 'whether the tiller self-centres',
    kind: 'bool',
    reset_required: false,
  },
  { path: 'sim.dt', tag: 'TUNABLE', unit: 's', doc: 'fixed timestep', kind: 'f64', reset_required: true },
]

describe('parameter schema', () => {
  it('produces one control per leaf field of parameters_json()', () => {
    const controls = buildSchema(VALUES, META)
    expect(controls.map((c) => c.path)).toEqual([
      'hull.loa',
      'hull.m_hull',
      'hull.sailor_pos_b.x',
      'hull.sailor_pos_b.y',
      'hull.sailor_pos_b.z',
      'sail.area',
      'sail.cd0',
      'sail.mast_pos_b.x',
      'sail.mast_pos_b.y',
      'sail.mast_pos_b.z',
      'rudder.delta_r_max',
      'rudder.delta_r_self_centre',
      'sim.dt',
    ])
  })

  it('skips the one parameter that is not a scalar', () => {
    expect(leafPaths(VALUES).map((l) => l.path)).not.toContain('sim.integrator')
  })

  it('carries the correct type and tag on every control', () => {
    const by = Object.fromEntries(buildSchema(VALUES, META).map((c) => [c.path, c]))
    expect(by['hull.loa'].kind).toBe('f64')
    expect(by['hull.loa'].tag).toBe('KNOWN')
    expect(by['hull.loa'].unit).toBe('m')
    expect(by['rudder.delta_r_self_centre'].kind).toBe('bool')
    expect(by['rudder.delta_r_self_centre'].tag).toBe('TUNABLE')
    expect(by['sail.cd0'].tag).toBe('ASSUMED')
    expect(by['sim.dt'].resetRequired).toBe(true)
    expect(by['sail.area'].resetRequired).toBe(false)
    for (const control of buildSchema(VALUES, META)) {
      expect(PARAM_TAGS, control.path).toContain(control.tag)
    }
  })

  it('reads the value at every leaf, flags as 0/1', () => {
    const by = Object.fromEntries(buildSchema(VALUES, META).map((c) => [c.path, c]))
    expect(by['hull.loa'].value).toBe(4.23)
    expect(by['hull.sailor_pos_b.z'].value).toBe(0.35)
    expect(by['rudder.delta_r_self_centre'].value).toBe(1)
  })

  it('still renders a control for a leaf the metadata has not caught up with', () => {
    const controls = buildSchema({ sail: { brand_new: 1.5 } }, META)
    expect(controls).toHaveLength(1)
    expect(controls[0].path).toBe('sail.brand_new')
    expect(controls[0].tag).toBe('')
    expect(controls[0].resetRequired).toBe(false)
  })

  it('buckets by group, preserving declaration order', () => {
    expect(groupSchema(buildSchema(VALUES, META)).map((g) => g.group)).toEqual([
      'hull',
      'sail',
      'rudder',
      'sim',
    ])
  })

  it('picks a step from the value magnitude', () => {
    expect(stepFor(20000)).toBe(100)
    expect(stepFor(7.06)).toBe(0.01)
    expect(stepFor(0.06)).toBe(0.0001)
    expect(stepFor(0)).toBe(0.01)
  })

  it('has no hand-written field list in the source', () => {
    // The panel is generated from the parameter tree (brief §31, task 8.5).
    // The only literal paths allowed in these two files are brief §31's own
    // example list, which is a requirement being checked rather than a
    // catalogue being mirrored — so it is fenced off in its own constant and
    // the count of literal paths anywhere else must be zero.
    const schema = readFileSync('src/ui/parameterSchema.ts', 'utf8')
    const panel = readFileSync('src/ui/ParameterPanel.tsx', 'utf8')
    const fenced = schema.slice(schema.indexOf('BRIEF_31_PARAMETERS'))
    const elsewhere = schema.slice(0, schema.indexOf('BRIEF_31_PARAMETERS'))
    const dottedPathLiteral = /'(hull|inertia|resistance|sail|board|rudder|sheet|stability|sim)\.[a-z0-9_.]+'/g

    expect(elsewhere.match(dottedPathLiteral), 'parameterSchema.ts outside the §31 list').toBeNull()
    expect(panel.match(dottedPathLiteral), 'ParameterPanel.tsx').toBeNull()
    // …and the fenced list really is the brief's, not a catalogue in disguise.
    expect(fenced.match(dottedPathLiteral)?.length).toBe(BRIEF_31_PARAMETERS.length)
    expect(BRIEF_31_PARAMETERS.length).toBeLessThan(20)
  })
})

/**
 * v2 section 09, task 9.2 — the actuator metadata the touch gauges read.
 *
 * ## The finding, recorded here because the task told it to be
 *
 * Task 9.2 owns `crates/sailgym-wasm/src/lib.rs` and is told to "expose only
 * missing limits/rates needed by gauges". **Nothing was missing.**
 * `Sim::parameters_json()` serialises the whole `BoatParameters` catalogue, so
 * `rudder.delta_r_max`, `rudder.delta_r_rate_max`,
 * `rudder.delta_r_return_rate`, `sheet.sheet_haul_rate`,
 * `sheet.sheet_ease_rate` and `sheet.sheet_release_rate` all already crossed
 * the boundary — exactly as section 01 found for the six geometry fields it
 * needed (`docs/v2/progress/01-handoff.md` §1, task 1.5). **No Rust changed**,
 * and no parallel type was created to fit an `Owns:` list: task 9.1's
 * `RenderParams` in `sim/useSimulation.ts` gained the matching *view* fields,
 * which is where the PRD says they belong.
 *
 * What is left for this task is the part that can go wrong: proving the paths
 * are the ones the core accepts, that the gauges compute their reference times
 * from them, and that no catalogue value is restated in the touch logic.
 */

/** Every catalogue path the section-09 gauges and reference times read. */
const ACTUATOR_PATHS = [
  'rudder.delta_r_max',
  'rudder.delta_r_rate_max',
  'rudder.delta_r_return_rate',
  'sheet.l_sheet_min',
  'sheet.l_sheet_max',
  'sheet.sheet_haul_rate',
  'sheet.sheet_ease_rate',
  'sheet.sheet_release_rate',
] as const

describe('actuator metadata (task 9.2)', () => {
  it('every gauge path is a leaf of a parameters_json() document', () => {
    // `leafPaths` is the same walk the parameter panel uses, so a path the
    // gauges read is a path `Simulation::set_parameter` accepts and the panel
    // offers. Spelling them differently on the two sides is the drift this
    // catches.
    const paths = new Set(leafPaths(ilcaParams()).map((l) => l.path))
    for (const path of ACTUATOR_PATHS) {
      expect(paths, path).toContain(path)
    }
  })

  it('the WASM surface gained no field for them: parameters_json() already carried them', () => {
    // The whole catalogue, in one coarse-grained call (brief §24). If this
    // ever becomes a hand-picked subset, the gauges start lying the moment a
    // parameter is added.
    const lib = readFileSync('../crates/sailgym-wasm/src/lib.rs', 'utf8')
    expect(lib).toContain('serde_json::to_string(self.inner.params())')
    // …and no per-field accessor crept in beside it (F8.2, brief §24).
    for (const path of ACTUATOR_PATHS) {
      const field = path.split('.')[1]
      expect(lib, `a per-field ${field} accessor`).not.toMatch(
        new RegExp(`pub fn ${field}\\s*\\(`),
      )
    }
  })

  it('reference times are (max − min) / rate, from the catalogue', () => {
    const p = ilcaParams()
    const t = fullTravel(p)
    const span = p.sheet.l_sheet_max - p.sheet.l_sheet_min
    expect(t.sheetHaul).toBeCloseTo(span / p.sheet.sheet_haul_rate, 12)
    expect(t.sheetEase).toBeCloseTo(span / p.sheet.sheet_ease_rate, 12)
    expect(t.sheetRelease).toBeCloseTo(span / p.sheet.sheet_release_rate, 12)
    expect(t.rudderStopToStop).toBeCloseTo(
      (2 * p.rudder.delta_r_max) / p.rudder.delta_r_rate_max,
      12,
    )
    expect(t.rudderCentreToStop).toBeCloseTo(t.rudderStopToStop / 2, 12)
  })

  it('a live parameter edit moves every one of them', () => {
    // brief §31 makes the catalogue editable while the boat sails, so the
    // reference times are a function of the parameters and of nothing else.
    // Halving a rate doubles its time; widening the travel widens all three.
    const base = fullTravel(ilcaParams())
    const slower = fullTravel(
      ilcaParams({ sheet: { sheet_haul_rate: 0.75 }, rudder: { delta_r_rate_max: 1.045 } }),
    )
    expect(slower.sheetHaul).toBeCloseTo(2 * base.sheetHaul, 9)
    expect(slower.rudderStopToStop).toBeCloseTo(2 * base.rudderStopToStop, 9)

    const longer = fullTravel(ilcaParams({ sheet: { l_sheet_max: 8.5 } }))
    expect(longer.sheetHaul).toBeGreaterThan(base.sheetHaul)
    expect(longer.sheetEase).toBeGreaterThan(base.sheetEase)
    expect(longer.sheetRelease).toBeGreaterThan(base.sheetRelease)

    // A rate that has been edited to zero is reported as unreachable rather
    // than as a division by zero shown to the player.
    expect(fullTravel(ilcaParams({ sheet: { sheet_haul_rate: 0 } })).sheetHaul).toBe(Infinity)
  })

  it('no catalogue value and no superseded target time appears in the touch logic', () => {
    // The PRD names `0.9`, `4.5`, and the 2.4 s haul / 0.6 s release figures of
    // the superseded position-control proposal. None of them may be assumed:
    // `l_sheet_min` moved in v2 F18.1c and would have made the first one wrong
    // on the spot.
    const forbidden = /\b(0\.9|4\.5|2\.4|0\.6|1\.0404326023342405)\b/
    for (const file of [
      'src/ui/TouchControls.tsx',
      'src/sim/touchInput.ts',
      'src/sim/controls.ts',
    ]) {
      const source = readFileSync(file, 'utf8')
      for (const [i, line] of source.split('\n').entries()) {
        // Prose in a doc comment may *name* the superseded figures in order to
        // say they are not targets; code may not contain them.
        const isComment = /^\s*(\*|\/\/|\/\*)/.test(line)
        if (!isComment) {
          expect(forbidden.test(line), `${file}:${i + 1}: ${line.trim()}`).toBe(false)
        }
      }
    }
  })

  it('the panel still has no hand-written field list', () => {
    // Task 9.2 must not have turned the schema into a catalogue in disguise.
    const schema = readFileSync('src/ui/parameterSchema.ts', 'utf8')
    const dotted = /'(hull|inertia|resistance|sail|board|rudder|sheet|stability|sim)\.[a-z0-9_.]+'/g
    const elsewhere = schema.slice(0, schema.indexOf('BRIEF_31_PARAMETERS'))
    expect(elsewhere.match(dotted)).toBeNull()
    // The list above lives in this test file, where it is a claim being
    // checked rather than a second catalogue the application reads.
    expect(ACTUATOR_PATHS.length).toBeLessThan(20)
  })
})
