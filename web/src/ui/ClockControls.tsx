import { SPEEDS, type ClockState, type SpeedMultiplier } from '../sim/clock'

/**
 * Pause / resume, reset, single-step and the four speeds (brief §22).
 *
 * Every control carries a stable `data-testid` — the E2E suite drives the
 * simulator through these, so the ids are a contract.
 */
export interface ClockControlsProps {
  state: ClockState
  onToggleRunning: () => void
  onReset: () => void
  onSingleStep: () => void
  onSetSpeed: (s: SpeedMultiplier) => void
}

export function ClockControls({
  state,
  onToggleRunning,
  onReset,
  onSingleStep,
  onSetSpeed,
}: ClockControlsProps) {
  return (
    <div data-testid="clock-controls" style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
      <button
        type="button"
        data-testid="clock-pause"
        data-running={state.running}
        onClick={onToggleRunning}
      >
        {state.running ? 'Pause' : 'Resume'}
      </button>
      <button type="button" data-testid="clock-reset" onClick={onReset}>
        Reset
      </button>
      <button
        type="button"
        data-testid="clock-step"
        onClick={onSingleStep}
        disabled={state.running}
        title="One physics step; only while paused"
      >
        Step
      </button>
      {SPEEDS.map((s) => (
        <button
          key={s}
          type="button"
          data-testid={`clock-speed-${s}x`}
          data-active={state.speed === s}
          onClick={() => onSetSpeed(s)}
          style={{ fontWeight: state.speed === s ? 700 : 400 }}
        >
          {s}×
        </button>
      ))}
      <span data-testid="clock-time" data-sim-time={state.simTime} data-steps={state.stepsTaken}>
        t = {state.simTime.toFixed(2)} s
      </span>
    </div>
  )
}
