import type { Diagnostics } from '../sim/diagnostics'

/**
 * Grouped numeric readouts for the whole diagnostics record (task 8.4).
 *
 * ## Every field, always
 *
 * The groups below are a *presentation* order, not a field list. The panel
 * renders one row per key of the record it was handed, and any key the groups
 * do not mention lands in "Other" — so a field added in `diagnostics.rs`
 * appears here the moment the WASM package is rebuilt, with no edit to this
 * file. `debug.spec.ts` enumerates the TypeScript type's keys and asserts a
 * `[data-testid="diag-<key>"]` for each; that test can only be satisfied by
 * enumerating, never by keeping a list in step by hand.
 *
 * Nothing is computed here. Values are formatted and nothing else (F8).
 */

export interface DiagGroup {
  title: string
  keys: readonly string[]
}

/** brief §30's own grouping, as far as it gives one. */
export const DIAG_GROUPS: readonly DiagGroup[] = [
  { title: 'Clock', keys: ['t', 'steps'] },
  {
    title: 'Environment',
    keys: [
      'true_wind_world',
      'true_wind_body',
      'apparent_wind_body',
      'apparent_wind_speed',
      'apparent_wind_angle',
    ],
  },
  {
    title: 'Motion',
    keys: [
      'velocity_body',
      'speed_over_ground',
      'course_over_ground',
      'acceleration_body',
      'yaw_rate',
      'roll_rate',
      'leeway_angle',
    ],
  },
  {
    title: 'Forces (B)',
    keys: ['sail', 'board', 'rudder', 'hull', 'sheet', 'sheet_hull', 'total_force_h'],
  },
  {
    title: 'Moments',
    keys: ['yaw_moment', 'heeling_moment', 'righting_moment', 'boom_moment'],
  },
  {
    title: 'Geometry (B)',
    keys: [
      'sail_ce_b',
      'board_centre_b',
      'rudder_centre_b',
      'sheet_attach_b',
      'sheet_block_b',
    ],
  },
  {
    title: 'Coefficients',
    keys: [
      'alpha_sail',
      'cl_sail',
      'cd_sail',
      'alpha_board',
      'cl_board',
      'cd_board',
      'alpha_rudder',
      'cl_rudder',
      'cd_rudder',
    ],
  },
  {
    title: 'Rig',
    keys: ['beta', 'beta_dot', 'sheet_tension', 'sheet_rope_length', 'sheet_extension'],
  },
  { title: 'Stability', keys: ['gz', 'heel_deg', 'capsize'] },
  {
    title: 'Health',
    keys: [
      'energy_kinetic',
      'energy_roll_potential',
      'energy_sheet_elastic',
      'hull_model_warning',
    ],
  },
]

/** A number with enough digits to be useful and few enough to be read. */
function num(v: number): string {
  if (!Number.isFinite(v)) {
    return String(v)
  }
  const a = Math.abs(v)
  if (a === 0) return '0'
  if (a >= 1e5 || a < 1e-3) return v.toExponential(3)
  if (a >= 100) return v.toFixed(1)
  if (a >= 1) return v.toFixed(3)
  return v.toFixed(4)
}

/** One value, flattened to a line. Vectors, loads and records all nest. */
function format(value: unknown): string {
  if (typeof value === 'number') return num(value)
  if (typeof value === 'boolean') return value ? 'yes' : 'no'
  if (value === null || value === undefined) return '—'
  if (typeof value === 'object') {
    return Object.entries(value as Record<string, unknown>)
      .map(([k, v]) => `${k} ${format(v)}`)
      .join('  ')
  }
  return String(value)
}

/** `Forces (B)` → `forces-b`; the group's stable `data-testid` suffix. */
function slug(title: string): string {
  return title
    .toLowerCase()
    .replace(/[^a-z]+/g, '-')
    .replace(/^-|-$/g, '')
}

function Row({ name, value }: { name: string; value: unknown }) {
  return (
    <div
      data-testid={`diag-${name}`}
      data-value={typeof value === 'object' ? JSON.stringify(value) : String(value)}
      style={{ display: 'flex', gap: 8, lineHeight: 1.45 }}
    >
      <span style={{ flex: '0 0 170px', color: '#556' }}>{name}</span>
      <span style={{ flex: 1, fontVariantNumeric: 'tabular-nums' }}>{format(value)}</span>
    </div>
  )
}

export function DebugPanel({ diagnostics }: { diagnostics: Diagnostics | null }) {
  if (diagnostics === null) {
    return null
  }
  const record = diagnostics as unknown as Record<string, unknown>
  const keys = Object.keys(record)
  const grouped = new Set(DIAG_GROUPS.flatMap((g) => g.keys))
  const ungrouped = keys.filter((k) => !grouped.has(k))
  const groups: DiagGroup[] =
    ungrouped.length === 0
      ? [...DIAG_GROUPS]
      : [...DIAG_GROUPS, { title: 'Other', keys: ungrouped }]

  return (
    <section
      data-testid="debug-panel"
      data-fields={keys.length}
      style={{ border: '1px solid #ccd', borderRadius: 4, padding: 8 }}
    >
      <strong>Diagnostics</strong>
      {diagnostics.hull_model_warning && (
        <p
          data-testid="hull-model-warning"
          style={{
            margin: '6px 0',
            padding: 6,
            background: '#fff3cd',
            border: '1px solid #e0c060',
            borderRadius: 3,
          }}
        >
          <strong>Hull model extrapolating.</strong> Above ≈ 5 m/s the F6.6 resistance model has
          no planing regime and over-predicts drag (R6, brief §15). The hull force and anything
          derived from it are not validated data here.
        </p>
      )}
      {groups.map((group) => {
        const present = group.keys.filter((k) => k in record)
        if (present.length === 0) {
          return null
        }
        return (
          <div key={group.title} data-testid={`diag-group-${slug(group.title)}`}>
            <div style={{ marginTop: 6, color: '#334', fontWeight: 600 }}>{group.title}</div>
            {present.map((k) => (
              <Row key={k} name={k} value={record[k]} />
            ))}
          </div>
        )
      })}
    </section>
  )
}
