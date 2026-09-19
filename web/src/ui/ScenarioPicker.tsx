import { memo } from 'react'

import type { ScenarioSummary } from '../sim/scenarioTypes'

/**
 * The six shipped scenarios of brief §32 (task 9.2).
 *
 * The list comes from the core through `Sim.scenarios_json()` — this
 * component is handed the rows and never authors one. Picking a scenario
 * reloads the simulation from that document: parameters, initial state, wind
 * and seed all together, which is the only way a scenario means anything.
 *
 * `data-testid="scenario-picker"` and `data-scenario` are a contract with
 * `scenarios.spec.ts`.
 */
export interface ScenarioPickerProps {
  scenarios: readonly ScenarioSummary[]
  current: string
  onSelect: (id: string) => void
  disabled?: boolean
}

export const ScenarioPicker = memo(function ScenarioPicker({
  scenarios,
  current,
  onSelect,
  disabled = false,
}: ScenarioPickerProps) {
  const chosen = scenarios.find((s) => s.id === current)
  return (
    <label
      data-testid="scenario-picker"
      data-scenario={current}
      data-count={scenarios.length}
      style={{ display: 'flex', gap: 6, alignItems: 'center' }}
      title={chosen?.description ?? ''}
    >
      Scenario:{' '}
      <select
        data-testid="scenario-select"
        value={current}
        disabled={disabled || scenarios.length === 0}
        onChange={(e) => onSelect(e.target.value)}
      >
        {/* A run started from an ad-hoc fixture is not in the shipped list;
            show it rather than silently displaying the wrong selection. */}
        {chosen === undefined && (
          <option key={current} value={current}>
            {current === '' ? '—' : current}
          </option>
        )}
        {scenarios.map((s) => (
          <option key={s.id} value={s.id}>
            {s.id}
          </option>
        ))}
      </select>
    </label>
  )
})
