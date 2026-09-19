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
