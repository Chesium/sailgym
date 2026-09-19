/**
 * The parameter panel's schema, derived rather than written (task 8.5).
 *
 * **There is no field list in this file, and there must never be one.** The
 * controls come from walking the object `Sim::parameters_json()` returns, and
 * the tags, units and documentation come from `Sim::parameter_meta_json()`,
 * which Rust derives from `parameters.rs`'s own source. Add a parameter to the
 * F7 catalogue and it appears in the panel on the next WASM build; delete one
 * and its control disappears. A hand-kept mirror of F7 in TypeScript would be
 * a second catalogue to keep in step, and F7 says there is one (F7, F8).
 */

/** One record of `parameter_meta_json()`, as Rust serialises `ParamMeta`. */
export interface ParamMeta {
  path: string
  /** `KNOWN`, `ASSUMED`, `TUNABLE` or `DEFERRED` (F7, brief §48). */
  tag: string
  unit: string
  doc: string
  /** `f64` or `bool`. */
  kind: string
  reset_required: boolean
}

export const PARAM_TAGS = ['KNOWN', 'ASSUMED', 'TUNABLE', 'DEFERRED'] as const
export type ParamTag = (typeof PARAM_TAGS)[number]

/** A leaf of the value tree: a dotted path and the number or flag at it. */
export interface ParamLeaf {
  path: string
  value: number
  kind: 'f64' | 'bool'
}

/** One rendered control. */
export interface ParamControl extends ParamLeaf {
  /** First path segment: `hull`, `sail`, `sim`, … */
  group: string
  /** The rest of the path, which is what the label shows. */
  name: string
  tag: string
  unit: string
  doc: string
  resetRequired: boolean
}

export interface ParamGroup {
  group: string
  controls: ParamControl[]
}

/**
 * Every editable leaf of a `parameters_json()` document, in declaration
 * order.
 *
 * A leaf is a number or a boolean. Strings are skipped: `sim.integrator` is
 * the only one, it is not a scalar, and `set_path` has never accepted it
 * (section 02 handoff §2.8). Nested objects recurse, which is exactly how
 * `hull.sailor_pos_b` becomes `hull.sailor_pos_b.{x,y,z}` — the same
 * expansion the Rust path table performs, arrived at from the JSON rather
 * than restated.
 */
export function leafPaths(values: unknown, prefix = ''): ParamLeaf[] {
  if (values === null || typeof values !== 'object') {
    return []
  }
  const out: ParamLeaf[] = []
  for (const [key, value] of Object.entries(values as Record<string, unknown>)) {
    const path = prefix === '' ? key : `${prefix}.${key}`
    if (typeof value === 'number') {
      out.push({ path, value, kind: 'f64' })
    } else if (typeof value === 'boolean') {
      out.push({ path, value: value ? 1 : 0, kind: 'bool' })
    } else if (value !== null && typeof value === 'object') {
      out.push(...leafPaths(value, path))
    }
  }
  return out
}

/**
 * The controls to render: one per leaf, decorated with its metadata.
 *
 * A leaf with no metadata record still gets a control — an untagged parameter
 * is a defect in `parameters.rs`, not a reason to hide the field — and it is
 * marked so it is visible as such.
 */
export function buildSchema(values: unknown, meta: readonly ParamMeta[]): ParamControl[] {
  const byPath = new Map(meta.map((m) => [m.path, m]))
  return leafPaths(values).map((leaf) => {
    const m = byPath.get(leaf.path)
    const dot = leaf.path.indexOf('.')
    return {
      ...leaf,
      group: dot < 0 ? leaf.path : leaf.path.slice(0, dot),
      name: dot < 0 ? leaf.path : leaf.path.slice(dot + 1),
      tag: m?.tag ?? '',
      unit: m?.unit ?? '',
      doc: m?.doc ?? '',
      // The metadata is authoritative when it exists; the `sim.` rule is the
      // same one `BoatParameters::set_path` applies, and is the fallback for
      // a leaf whose metadata has not arrived.
      resetRequired: m?.reset_required ?? leaf.path.startsWith('sim.'),
    }
  })
}

/** The same controls, bucketed by their first path segment, order preserved. */
export function groupSchema(controls: readonly ParamControl[]): ParamGroup[] {
  const groups: ParamGroup[] = []
  for (const control of controls) {
    let bucket = groups.find((g) => g.group === control.group)
    if (bucket === undefined) {
      bucket = { group: control.group, controls: [] }
      groups.push(bucket)
    }
    bucket.controls.push(control)
  }
  return groups
}

/**
 * A sensible step for a numeric input, from the value's own magnitude.
 *
 * Dragging `sheet.k_sheet` (2e4 N/m) in steps of 0.01 is useless, and so is
 * dragging `sail.cd0` (0.06) in steps of 100.
 */
export function stepFor(value: number): number {
  const a = Math.abs(value)
  if (a === 0) return 0.01
  return 10 ** (Math.floor(Math.log10(a)) - 2)
}

/**
 * brief §31's own examples, as dotted paths.
 *
 * This **is** a literal list, and deliberately so: it is the brief's
 * requirement, checked against the generated panel by `params.spec.ts`. It
 * names nothing the panel is built from — if a path here stops existing, the
 * test fails and someone reads the brief again.
 *
 * `wind magnitude/direction` and `wind-field variation amplitude` are not in
 * this list because the wind is not in F7: it is scenario configuration
 * (`WindConfig`, section 03 handoff §2.3) and the panel edits it through
 * `set_wind`, not `set_parameter`. `BRIEF_31_WIND` names those three.
 */
export const BRIEF_31_PARAMETERS: readonly { example: string; path: string }[] = [
  { example: 'boat mass', path: 'hull.m_hull' },
  { example: 'sailor mass', path: 'hull.m_sailor' },
  { example: 'sail area', path: 'sail.area' },
  { example: 'drag coefficient', path: 'sail.cd0' },
  { example: 'damping coefficient', path: 'resistance.x_uu' },
  { example: 'damping coefficient (roll)', path: 'resistance.k_p' },
  { example: 'sheet stiffness', path: 'sheet.k_sheet' },
  { example: 'sheet damping', path: 'sheet.c_sheet' },
  { example: 'maximum rudder angle', path: 'rudder.delta_r_max' },
  { example: 'righting-moment parameter (GM)', path: 'stability.gm' },
  { example: 'righting-moment parameter (GZ max)', path: 'stability.gz_max' },
  { example: 'righting-moment parameter (peak angle)', path: 'stability.phi_peak' },
  { example: 'righting-moment parameter (vanishing angle)', path: 'stability.phi_vanish' },
]

/** The three brief §31 examples that live in the wind configuration. */
export const BRIEF_31_WIND: readonly { example: string; field: string }[] = [
  { example: 'wind magnitude', field: 'speed' },
  { example: 'wind direction', field: 'bearing_deg' },
  { example: 'wind-field variation amplitude', field: 'variation' },
]
