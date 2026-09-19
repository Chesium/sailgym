import { memo, useCallback, useEffect, useState } from 'react'

import {
  BRIEF_31_WIND,
  buildSchema,
  groupSchema,
  stepFor,
  type ParamControl,
  type ParamMeta,
} from './parameterSchema'
import type { SimHandle } from '../sim/loadWasm'

/**
 * The collapsible engineering panel of brief §31 (task 8.5).
 *
 * The controls are **generated** from `parameters_json()` and
 * `parameter_meta_json()`; see `parameterSchema.ts` for why there is no field
 * list anywhere on this side of the boundary.
 *
 * Three things this panel is responsible for beyond editing:
 *
 * * **Showing the F7 tag.** A KNOWN parameter is editable, because refusing to
 *   let an engineer change the sail area would be absurd, but it is marked:
 *   changing one means you are no longer simulating an ILCA.
 * * **Surfacing a rejected edit.** `Sim::set_parameter` validates the whole
 *   catalogue and re-fits the F6.7 `GZ` curve before committing, so an
 *   impossible stability group comes back as an error instead of quietly
 *   producing a boat with no righting arm. The message is shown; the slider
 *   snaps back to the value the core still holds.
 * * **Saying when a reset is owed.** `set_parameter` returns whether the edit
 *   invalidated simulation continuity (F8.2). The panel shows a badge and a
 *   reset button rather than letting the run carry on meaning something else
 *   (brief §31).
 */

const TAG_STYLE: Record<string, { background: string; color: string; title: string }> = {
  KNOWN: {
    background: '#fde2e0',
    color: '#8a2a20',
    title: 'Published ILCA/class data. Change it and you are no longer simulating an ILCA.',
  },
  ASSUMED: {
    background: '#e6eef7',
    color: '#28527a',
    title: 'A physically motivated estimate: plausible, unvalidated.',
  },
  TUNABLE: {
    background: '#e6f4e6',
    color: '#276127',
    title: 'Expected to be adjusted by playtesting or fitting.',
  },
  DEFERRED: {
    background: '#eee',
    color: '#555',
    title: 'The hook exists but is unused in v1.',
  },
}

function Tag({ tag }: { tag: string }) {
  const style = TAG_STYLE[tag] ?? { background: '#fff0f0', color: '#a00', title: 'Untagged' }
  return (
    <span
      data-testid={`tag-${tag === '' ? 'none' : tag}`}
      title={style.title}
      style={{
        background: style.background,
        color: style.color,
        borderRadius: 3,
        padding: '0 4px',
        fontSize: 10,
        letterSpacing: 0.3,
      }}
    >
      {tag === '' ? 'UNTAGGED' : tag}
    </span>
  )
}

export interface ParameterPanelProps {
  /** Run something against the live `Sim`, or `null` before it is ready. */
  withSim: <T>(fn: (sim: SimHandle) => T) => T | null
  ready: boolean
  open: boolean
  onOpenChange: (open: boolean) => void
  resetRequired: boolean
  onResetRequired: (required: boolean) => void
  /** Restart the run, which is what clears a reset-required edit. */
  onReset: () => void
}

interface PanelState {
  controls: ParamControl[]
  meta: ParamMeta[]
  /** The live `WindConfig`, which is scenario configuration, not F7. */
  wind: Record<string, unknown>
}

const EMPTY: PanelState = { controls: [], meta: [], wind: {} }

/**
 * Units for the three wind fields brief §31 asks for.
 *
 * The wind is **not** in the F7 catalogue — it is a property of the scenario,
 * not of the boat (section 03 handoff §2.3) — so it has no `ParamMeta` and is
 * edited through `set_wind` rather than `set_parameter`. It is in this panel
 * because brief §31 lists it, and brief §31 is a list of what an engineer must
 * be able to change, not of where the value happens to live.
 */
const WIND_UNITS: Record<string, string> = {
  speed: 'm/s',
  bearing_deg: 'deg, FROM',
  variation: '0..1',
}

/**
 * Cautions attached to a whole group.
 *
 * `sim` carries one because the section 06 and 07 handoffs both asked for it:
 * `dt = 0.01` is inside brief §21's range but the boom mode gains energy there
 * in the configuration where the sheet's own damping vanishes (section 06 §3,
 * section 07 §8.4). The value is offered — F7 tags it TUNABLE — but not as an
 * equal option.
 */
const GROUP_NOTES: Record<string, string> = {
  sim:
    'dt = 0.005 is the tested value. brief §21 allows up to 0.01, but the boom mode ' +
    'loses margin there and gains energy with the sheet hauled hard in (section 06 ' +
    'handoff §3). Raise it only deliberately.',
}

/**
 * Memoised: none of its props is a simulation quantity, so there is no reason
 * for eighty-nine number inputs to be reconciled sixty times a second just
 * because the boat moved. It re-reads the catalogue from the core when an
 * edit lands, which is the only time the catalogue changes.
 */
export const ParameterPanel = memo(function ParameterPanel({
  withSim,
  ready,
  open,
  onOpenChange,
  resetRequired,
  onResetRequired,
  onReset,
}: ParameterPanelProps) {
  const [state, setState] = useState<PanelState>(EMPTY)
  const [error, setError] = useState<string | null>(null)
  const [edited, setEdited] = useState<readonly string[]>([])

  /** Re-read the catalogue from the core. The core is the only source. */
  const refresh = useCallback(
    (meta?: ParamMeta[]) => {
      const next = withSim((sim) => {
        const values: unknown = JSON.parse(sim.parameters_json() as string)
        const records =
          meta ?? (JSON.parse(sim.parameter_meta_json() as string) as ParamMeta[])
        const wind = JSON.parse(sim.wind_json() as string) as Record<string, unknown>
        return { controls: buildSchema(values, records), meta: records, wind }
      })
      if (next !== null) {
        setState(next)
      }
    },
    [withSim],
  )

  useEffect(() => {
    if (ready) {
      refresh()
    }
  }, [ready, refresh])

  const commit = (control: ParamControl, raw: number) => {
    if (!Number.isFinite(raw)) {
      setError(`${control.path}: not a number`)
      return
    }
    const outcome = withSim((sim) => {
      try {
        return { reset: sim.set_parameter(control.path, raw), message: null as string | null }
      } catch (cause: unknown) {
        return { reset: false, message: cause instanceof Error ? cause.message : String(cause) }
      }
    })
    if (outcome === null) {
      return
    }
    if (outcome.message !== null) {
      // Rejected: the core still holds the old value, so re-reading it is what
      // snaps the control back. Nothing is left in a half-applied state.
      setError(outcome.message)
      refresh(state.meta)
      return
    }
    setError(null)
    setEdited((e) => (e.includes(control.path) ? e : [...e, control.path]))
    if (outcome.reset) {
      onResetRequired(true)
    }
    refresh(state.meta)
  }

  const commitWind = (field: string, raw: number) => {
    if (!Number.isFinite(raw)) {
      setError(`wind.${field}: not a number`)
      return
    }
    const message = withSim((sim) => {
      try {
        sim.set_wind(JSON.stringify({ ...state.wind, [field]: raw }))
        return null
      } catch (cause: unknown) {
        return cause instanceof Error ? cause.message : String(cause)
      }
    })
    if (message !== null && message !== undefined) {
      setError(message)
    } else {
      setError(null)
      setEdited((e) => (e.includes(`wind.${field}`) ? e : [...e, `wind.${field}`]))
    }
    refresh(state.meta)
  }

  const resetToDefaults = () => {
    withSim((sim) => sim.reset_parameters())
    setEdited([])
    setError(null)
    refresh(state.meta)
  }

  const groups = groupSchema(state.controls)

  return (
    <section
      data-testid="parameter-panel"
      data-open={open ? 'true' : 'false'}
      data-controls={state.controls.length}
      data-edited={edited.length}
      style={{ border: '1px solid #ccd', borderRadius: 4, padding: 8 }}
    >
      <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
        <button
          type="button"
          data-testid="parameter-panel-toggle"
          onClick={() => onOpenChange(!open)}
          style={{ flex: 1, textAlign: 'left', font: 'inherit', fontWeight: 600 }}
        >
          {open ? '▾' : '▸'} Parameters ({state.controls.length})
        </button>
        <button type="button" data-testid="parameters-reset-defaults" onClick={resetToDefaults}>
          Reset to ILCA defaults
        </button>
      </div>

      {resetRequired && (
        <div
          data-testid="reset-required-badge"
          style={{
            margin: '6px 0',
            padding: 6,
            background: '#fff3cd',
            border: '1px solid #e0c060',
            borderRadius: 3,
            display: 'flex',
            gap: 8,
            alignItems: 'center',
          }}
        >
          <span style={{ flex: 1 }}>
            <strong>Reset required.</strong> This edit changes what every recorded step means
            (F9, brief §33); the run since it is not comparable with the run before it.
          </span>
          <button
            type="button"
            data-testid="reset-required-reset"
            onClick={() => {
              onReset()
              onResetRequired(false)
            }}
          >
            Reset run
          </button>
        </div>
      )}

      {error !== null && (
        <p
          data-testid="parameter-error"
          role="alert"
          style={{
            margin: '6px 0',
            padding: 6,
            background: '#fde2e0',
            border: '1px solid #d08080',
            borderRadius: 3,
            color: '#8a2a20',
          }}
        >
          {error}
        </p>
      )}

      {open && (
        <div data-testid="parameter-groups" style={{ marginTop: 6 }}>
          <details data-testid="param-group-wind" open>
            <summary style={{ cursor: 'pointer', fontWeight: 600, margin: '4px 0' }}>
              wind ({BRIEF_31_WIND.length}) — scenario, not F7
            </summary>
            {BRIEF_31_WIND.map(({ field }) => {
              const value = typeof state.wind[field] === 'number' ? (state.wind[field] as number) : 0
              return (
                <div
                  key={field}
                  data-testid={`param-wind.${field}`}
                  data-value={value}
                  data-tag="TUNABLE"
                  data-kind="f64"
                  data-reset-required="false"
                  style={{ display: 'flex', gap: 6, alignItems: 'center', lineHeight: 1.6 }}
                >
                  <span style={{ flex: '1 1 150px' }}>{field}</span>
                  <Tag tag="TUNABLE" />
                  <input
                    type="number"
                    data-testid={`param-input-wind.${field}`}
                    value={value}
                    step={stepFor(value)}
                    onChange={(e) => commitWind(field, Number(e.target.value))}
                    style={{ width: 110, fontVariantNumeric: 'tabular-nums' }}
                  />
                  <span style={{ flex: '0 0 74px', color: '#667', fontSize: 11 }}>
                    {WIND_UNITS[field]}
                  </span>
                </div>
              )
            })}
          </details>
          {groups.map((group) => (
            <details key={group.group} data-testid={`param-group-${group.group}`} open>
              <summary style={{ cursor: 'pointer', fontWeight: 600, margin: '4px 0' }}>
                {group.group} ({group.controls.length})
              </summary>
              {GROUP_NOTES[group.group] !== undefined && (
                <p
                  data-testid={`param-note-${group.group}`}
                  style={{ margin: '2px 0 4px', color: '#8a6d1a', fontSize: 11 }}
                >
                  {GROUP_NOTES[group.group]}
                </p>
              )}
              {group.controls.map((control) => (
                <div
                  key={control.path}
                  data-testid={`param-${control.path}`}
                  data-value={control.value}
                  data-tag={control.tag}
                  data-kind={control.kind}
                  data-reset-required={control.resetRequired ? 'true' : 'false'}
                  title={control.doc}
                  style={{ display: 'flex', gap: 6, alignItems: 'center', lineHeight: 1.6 }}
                >
                  <span style={{ flex: '1 1 150px' }}>{control.name}</span>
                  <Tag tag={control.tag} />
                  {control.kind === 'bool' ? (
                    <input
                      type="checkbox"
                      data-testid={`param-input-${control.path}`}
                      checked={control.value !== 0}
                      onChange={(e) => commit(control, e.target.checked ? 1 : 0)}
                    />
                  ) : (
                    <input
                      type="number"
                      data-testid={`param-input-${control.path}`}
                      value={control.value}
                      step={stepFor(control.value)}
                      onChange={(e) => commit(control, Number(e.target.value))}
                      style={{ width: 110, fontVariantNumeric: 'tabular-nums' }}
                    />
                  )}
                  <span style={{ flex: '0 0 74px', color: '#667', fontSize: 11 }}>
                    {control.unit}
                  </span>
                </div>
              ))}
            </details>
          ))}
        </div>
      )}
    </section>
  )
})
