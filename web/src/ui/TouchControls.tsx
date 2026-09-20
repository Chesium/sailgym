import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { DEFAULT_INPUT, IDLE_TOUCH, type InputConfig, type TouchCommand } from '../sim/controls'
import { radiansToDegrees } from '../sim/units'
import type { Snapshot } from '../sim/snapshot'
import {
  IDLE_TOUCH_INPUT,
  reduceTouchInput,
  touchCommand,
  type PadId,
  type TouchEventType,
  type TouchInputState,
} from '../sim/touchInput'
import type { RenderParams } from '../sim/useSimulation'

/**
 * The on-screen helm, trim and release (v2 section 09, task 9.3).
 *
 * Two relative rate pads and a hold-to-release button, sized for a thumb, with
 * the boat's **actual** rudder angle and sheet length beside the **command**
 * each pad is sending. The two are deliberately drawn as separate rows in
 * separate units — degrees and metres against a dimensionless `[−1, 1]` — so
 * that the lag between asking for something and the boat doing it reads as
 * what it is, and not as a position error the controls are failing to close
 * (RV54).
 *
 * ## What is not here
 *
 * No rate cap, no tension-dependent slip, no hand model, no ratchet, no cleat,
 * no angle controller, no vibration. Every one of those would be a second
 * plant living in TypeScript (RV56, v2 F18.2). What crosses the boundary is a
 * normalised command in `[−1, 1]`; what turns it into metres per second is
 * `sheet.sheet_haul_rate` and friends, in `parameters.rs`.
 *
 * ## Interaction
 *
 * All of it lives in the pure reducer `sim/touchInput.ts`, which is what
 * `tests/unit/touchInput.test.ts` drives with synthetic pointers: one pointer
 * per pad, a second pointer on the same pad ignored, the other pad still
 * usable, and `pointerup` / `pointercancel` / `lostpointercapture` / unmount
 * all ending a grab. This component only calls `setPointerCapture` and
 * forwards coordinates.
 *
 * ## Keyboard
 *
 * Every control has a keyboard equivalent already, on `window`, in
 * `sim/useSimulation.ts`: `A`/`←` and `D`/`→` steer, `Space` releases, `R`
 * resets. They are advertised through `aria-keyshortcuts` rather than
 * re-implemented here, because a second key handler would be a second writer
 * into the input state, which is exactly what task 9.1 exists to prevent.
 */
export interface TouchControlsProps {
  /** The F8.3 snapshot — the boat's **actual** state (F3). */
  snapshot: Snapshot
  /** The live F7 catalogue, as `parameters_json()` shapes it. */
  params: RenderParams
  /** Hand the composed pad command to the one input boundary (task 9.1). */
  onCommand: (cmd: TouchCommand) => void
  /** The Reset button; the same action as `R` and as the clock's reset. */
  onReset: () => void
  /**
   * `useSimulation`'s clear generation. Every change drops the pads' state.
   *
   * Blur, a hidden page, pause, reset, a scenario switch and entering replay
   * all bump it. Without it the composed command would go to zero while the
   * pad went on showing a full-scale helm and re-sending it on the next
   * `pointermove` — a stale held gesture resurrected by the very path that was
   * supposed to end it (RV53).
   *
   * A finger that is still down when this fires owns nothing until it lifts
   * and lands again, which is the intended reading of "the gesture is over".
   */
  clearSignal: number
  /** Gains and dead zones (brief §28). */
  config?: InputConfig
}

/**
 * The smallest a control may be, in CSS pixels.
 *
 * **Provenance.** 44 × 44 is the PRD's own rule for this section and the size
 * WCAG 2.2 SC 2.5.5 (Target Size, Enhanced) asks for; it is a presentation
 * constant and reaches nothing physical. The pads are given far more than the
 * minimum — see {@link PAD_MIN_PX} — because they carry a range rather than a
 * single hit.
 */
const TARGET_MIN_PX = 44

/**
 * The pads' minimum side, in CSS pixels.
 *
 * **Provenance.** `TOUCH_FULL_SCALE_PX` (in `sim/controls.ts`) puts full scale
 * at 64 px of horizontal travel for the helm and 96 px of vertical travel for
 * the sheet, so a pad must be able to contain a grab in the middle plus full
 * travel either side of it: 2 × 64 = 128. 132 is that, rounded up, and it is
 * three times {@link TARGET_MIN_PX}. A pointer captured by the pad keeps
 * reporting outside it, so this is about *comfort*, not about reachability —
 * full travel is still available from a grab in the corner.
 */
const PAD_MIN_PX = 132

/** Rows the gauges draw, so the actual state and the command never share one. */
type GaugeRow = 'actual' | 'command'

const STYLE = `
.sg-touch { display: grid; gap: 8px; }
.sg-touch-pads { display: flex; gap: 10px; flex-wrap: wrap; align-items: stretch; }
.sg-pad {
  position: relative;
  min-width: ${PAD_MIN_PX}px;
  min-height: ${PAD_MIN_PX}px;
  flex: 1 1 ${PAD_MIN_PX}px;
  border: 1px solid #94a7b5;
  border-radius: 10px;
  background: #f3f7fa;
  touch-action: none;
  user-select: none;
  -webkit-user-select: none;
  -webkit-tap-highlight-color: transparent;
  cursor: grab;
  display: grid;
  align-content: space-between;
  padding: 6px;
  font: inherit;
  color: #23323d;
}
.sg-pad[data-held='true'] { background: #e2eef6; border-color: #3d6f93; cursor: grabbing; }
.sg-pad-title { font-size: 12px; font-weight: 600; letter-spacing: 0.02em; }
.sg-pad-hint { font-size: 11px; color: #5a6b78; }
.sg-track { position: relative; height: 10px; border-radius: 5px; background: #d7e2ea; }
.sg-track-zero { position: absolute; top: -2px; bottom: -2px; width: 1px; background: #8fa3b1; }
.sg-track-fill { position: absolute; top: 0; bottom: 0; border-radius: 5px; }
.sg-track-fill[data-row='actual'] { background: #2b6c46; }
.sg-track-fill[data-row='command'] { background: #b4622a; }
.sg-gauge { display: grid; grid-template-columns: auto 1fr auto; gap: 8px; align-items: center; font-size: 12px; }
.sg-gauge-label { color: #45575f; white-space: nowrap; }
.sg-gauge-value { font-variant-numeric: tabular-nums; white-space: nowrap; }
.sg-touch-buttons { display: flex; gap: 10px; flex-wrap: wrap; }
.sg-touch-button {
  min-width: ${2 * TARGET_MIN_PX}px;
  min-height: ${TARGET_MIN_PX}px;
  border-radius: 10px;
  border: 1px solid #94a7b5;
  background: #f3f7fa;
  font: inherit;
  font-weight: 600;
  color: #23323d;
  touch-action: none;
  cursor: pointer;
}
.sg-touch-release { flex: 2 1 ${4 * TARGET_MIN_PX}px; }
.sg-touch-release[data-held='true'] { background: #b03030; border-color: #7d1f1f; color: #fff; }
.sg-touch :focus-visible,
.sg-touch-button:focus-visible { outline: 3px solid #1f6fb2; outline-offset: 2px; }
.sg-touch-travel { font-size: 11px; color: #5a6b78; }

/*
 * A short viewport — a phone in portrait with the browser chrome up, or any
 * phone in landscape — gets shorter pads, so the control deck cannot squeeze
 * the boat out of the screen (RV55).
 *
 * The pads keep their full range: the pointer is captured, so a drag that
 * leaves the element still reports, and full travel is available from a grab
 * anywhere in the pad. What shrinks is the comfortable area, not the control.
 * ${TARGET_MIN_PX} is the floor either way.
 */
@media (max-height: 760px) {
  .sg-pad { min-height: ${Math.max(TARGET_MIN_PX, Math.round(PAD_MIN_PX * 0.7))}px; }
  .sg-touch { gap: 6px; }
}
@media (max-height: 460px) {
  .sg-pad { min-height: ${Math.max(TARGET_MIN_PX, Math.round(PAD_MIN_PX * 0.45))}px; }
  .sg-touch-travel { display: none; }
}
`

export function TouchControls({
  snapshot,
  params,
  onCommand,
  onReset,
  clearSignal,
  config = DEFAULT_INPUT,
}: TouchControlsProps) {
  const state = useRef<TouchInputState>(IDLE_TOUCH_INPUT)
  const [command, setCommand] = useState<TouchCommand>(IDLE_TOUCH)
  const onCommandRef = useRef(onCommand)
  onCommandRef.current = onCommand

  const dispatch = useCallback(
    (type: TouchEventType, pad: PadId, pointerId: number, x: number, y: number) => {
      state.current = reduceTouchInput(state.current, { type, pad, pointerId, x, y })
      const next = touchCommand(state.current, config)
      setCommand(next)
      onCommandRef.current(next)
    },
    [config],
  )

  // Unmounting is a lost grab like any other: the pads are gone, so nothing
  // can send the `pointerup` that would have ended them (RV53).
  useEffect(
    () => () => {
      onCommandRef.current(IDLE_TOUCH)
    },
    [],
  )

  // The one clear path, arriving from `useSimulation`. It does **not** call
  // `onCommand`: the hook has already cleared its own copy, and calling back
  // into it would be this component telling the boundary something the
  // boundary just told it.
  useEffect(() => {
    state.current = IDLE_TOUCH_INPUT
    setCommand(IDLE_TOUCH)
  }, [clearSignal])

  const padHandlers = (pad: PadId) => ({
    onPointerDown: (e: React.PointerEvent<HTMLElement>) => {
      // Take the pointer only if the pad is free; `reduceTouchInput` ignores a
      // second `down`, and capturing for it would steal the events the first
      // finger's gesture needs.
      if (state.current[pad].pointerId !== null) {
        return
      }
      try {
        e.currentTarget.setPointerCapture(e.pointerId)
      } catch {
        // `setPointerCapture` throws `NotFoundError` for a pointer the browser
        // no longer considers active — a race against the finger lifting, and
        // a synthetic event dispatched by a test. Without capture the pad
        // simply loses events once the finger leaves it, which the reducer
        // already treats as the grab ending; an exception in a pointer handler
        // would instead take the page's error trap down with it.
      }
      // Suppress the browser's own gesture — scroll, pull-to-refresh, the
      // double-tap zoom — **only** now that this control owns the pointer.
      // Nothing outside a pad is prevented, so the world view keeps its own.
      e.preventDefault()
      dispatch('down', pad, e.pointerId, e.clientX, e.clientY)
    },
    onPointerMove: (e: React.PointerEvent<HTMLElement>) => {
      if (state.current[pad].pointerId !== e.pointerId) {
        return
      }
      e.preventDefault()
      dispatch('move', pad, e.pointerId, e.clientX, e.clientY)
    },
    onPointerUp: (e: React.PointerEvent<HTMLElement>) =>
      dispatch('up', pad, e.pointerId, e.clientX, e.clientY),
    onPointerCancel: (e: React.PointerEvent<HTMLElement>) =>
      dispatch('cancel', pad, e.pointerId, e.clientX, e.clientY),
    onLostPointerCapture: (e: React.PointerEvent<HTMLElement>) =>
      dispatch('lostcapture', pad, e.pointerId, e.clientX, e.clientY),
  })

  const rudderDeg = radiansToDegrees(snapshot.deltaR)
  const rudderMaxDeg = radiansToDegrees(params.rudder.delta_r_max)
  const sheetSpan = params.sheet.l_sheet_max - params.sheet.l_sheet_min
  const sheetFraction = sheetSpan > 0 ? (snapshot.lSheet - params.sheet.l_sheet_min) / sheetSpan : 0

  const travel = useMemo(() => fullTravel(params), [params])

  const helmHeld = state.current.rudder.pointerId !== null
  const sheetHeld = state.current.sheet.pointerId !== null
  const padsHeld =
    (helmHeld ? 1 : 0) + (sheetHeld ? 1 : 0) + (command.release ? 1 : 0)

  return (
    <section
      className="sg-touch"
      data-testid="touch-controls"
      data-pads-held={padsHeld}
      aria-label="Helm, mainsheet and release"
    >
      <style>{STYLE}</style>

      <div className="sg-touch-pads">
        <div
          className="sg-pad"
          data-testid="touch-pad-rudder"
          data-held={helmHeld ? 'true' : 'false'}
          role="slider"
          tabIndex={0}
          aria-label="Helm rate"
          aria-valuemin={-1}
          aria-valuemax={1}
          aria-valuenow={command.rudder ?? 0}
          aria-valuetext={`${describeCommand(command.rudder, 'port', 'starboard')}; rudder ${rudderDeg.toFixed(0)} degrees`}
          aria-keyshortcuts="A D ArrowLeft ArrowRight"
          {...padHandlers('rudder')}
        >
          <span className="sg-pad-title">Helm — drag ◂ ▸</span>
          <Track row="command" value={command.rudder ?? 0} signed />
          <span className="sg-pad-hint">rate command, not an angle · keys A / D</span>
        </div>

        <div
          className="sg-pad"
          data-testid="touch-pad-sheet"
          data-held={sheetHeld ? 'true' : 'false'}
          role="slider"
          tabIndex={0}
          aria-label="Mainsheet rate"
          aria-valuemin={-1}
          aria-valuemax={1}
          aria-valuenow={command.sheet ?? 0}
          aria-valuetext={`${describeCommand(command.sheet, 'haul in', 'ease out')}; sheet ${snapshot.lSheet.toFixed(2)} metres`}
          {...padHandlers('sheet')}
        >
          <span className="sg-pad-title">Sheet — drag ▾ in, ▴ out</span>
          <Track row="command" value={command.sheet ?? 0} signed />
          <span className="sg-pad-hint">rate command, not a length</span>
        </div>
      </div>

      <div className="sg-touch-buttons">
        <button
          type="button"
          className="sg-touch-button sg-touch-release"
          data-testid="touch-release"
          data-held={command.release ? 'true' : 'false'}
          aria-pressed={command.release}
          aria-label="Release the mainsheet — hold"
          aria-keyshortcuts="Space"
          {...padHandlers('release')}
        >
          {command.release ? 'RELEASING…' : 'HOLD TO RELEASE'}
        </button>
        <button
          type="button"
          className="sg-touch-button"
          data-testid="touch-reset"
          aria-keyshortcuts="R"
          onClick={onReset}
        >
          Reset
        </button>
      </div>

      {/* The boat's actual state. Different unit, different row, different
          colour from the command above — RV54. */}
      <div className="sg-gauge" data-testid="touch-gauge-rudder" data-value={snapshot.deltaR}>
        <span className="sg-gauge-label">Rudder (actual)</span>
        <Track
          row="actual"
          value={params.rudder.delta_r_max > 0 ? snapshot.deltaR / params.rudder.delta_r_max : 0}
          signed
        />
        <span className="sg-gauge-value">
          {rudderDeg >= 0 ? '▸' : '◂'} {Math.abs(rudderDeg).toFixed(1)}° of {rudderMaxDeg.toFixed(0)}°
        </span>
      </div>

      <div className="sg-gauge" data-testid="touch-gauge-sheet" data-value={snapshot.lSheet}>
        <span className="sg-gauge-label">Sheet (actual)</span>
        <Track row="actual" value={sheetFraction} />
        <span className="sg-gauge-value">
          {snapshot.lSheet.toFixed(2)} m of {params.sheet.l_sheet_max.toFixed(2)} m
        </span>
      </div>

      <div
        className="sg-gauge"
        data-testid="touch-command-rudder"
        data-value={command.rudder ?? 0}
        data-owned={command.rudder === null ? 'false' : 'true'}
      >
        <span className="sg-gauge-label">Helm command (rate)</span>
        <Track row="command" value={command.rudder ?? 0} signed />
        <span className="sg-gauge-value">{signedCommand(command.rudder)}</span>
      </div>

      <div
        className="sg-gauge"
        data-testid="touch-command-sheet"
        data-value={command.sheet ?? 0}
        data-owned={command.sheet === null ? 'false' : 'true'}
        data-release={command.release ? 'true' : 'false'}
      >
        <span className="sg-gauge-label">Sheet command (rate)</span>
        <Track row="command" value={command.sheet ?? 0} signed />
        <span className="sg-gauge-value">
          {command.release ? 'release' : signedCommand(command.sheet)}
        </span>
      </div>

      <div
        className="sg-touch-travel"
        data-testid="touch-travel"
        data-rudder-stop-to-stop={travel.rudderStopToStop}
        data-rudder-centre-to-stop={travel.rudderCentreToStop}
        data-sheet-haul={travel.sheetHaul}
        data-sheet-ease={travel.sheetEase}
        data-sheet-release={travel.sheetRelease}
        /* The catalogue values the five times above are computed from, so a
           browser test can check the arithmetic against the *live* parameters
           rather than against a second copy of F7 (task 9.2). Every one comes
           from `parameters_json()` by way of `RenderParams`. */
        data-rudder-max={params.rudder.delta_r_max}
        data-rudder-rate-max={params.rudder.delta_r_rate_max}
        data-sheet-min={params.sheet.l_sheet_min}
        data-sheet-max={params.sheet.l_sheet_max}
        data-haul-rate={params.sheet.sheet_haul_rate}
        data-ease-rate={params.sheet.sheet_ease_rate}
        data-release-rate={params.sheet.sheet_release_rate}
      >
        Full travel at full command: helm {travel.rudderStopToStop.toFixed(2)} s stop to stop ·
        sheet {travel.sheetHaul.toFixed(2)} s in, {travel.sheetEase.toFixed(2)} s out,{' '}
        {travel.sheetRelease.toFixed(2)} s released
      </div>
    </section>
  )
}

/**
 * One bar.
 *
 * `signed` draws from the centre, which is what a rate command is: zero is a
 * position on the track, not an empty bar. An unsigned track fills from the
 * left and is used for the sheet's length, which has no natural centre.
 */
function Track({ row, value, signed = false }: { row: GaugeRow; value: number; signed?: boolean }) {
  const v = Math.max(-1, Math.min(1, value))
  const left = signed ? 50 + Math.min(0, v) * 50 : 0
  const width = signed ? Math.abs(v) * 50 : Math.max(0, Math.min(1, value)) * 100
  return (
    <span className="sg-track" aria-hidden="true">
      {signed && <span className="sg-track-zero" style={{ left: '50%' }} />}
      <span
        className="sg-track-fill"
        data-row={row}
        style={{ left: `${left}%`, width: `${width}%` }}
      />
    </span>
  )
}

/** The five reference times, in seconds. See {@link fullTravel}. */
export interface FullTravel {
  rudderStopToStop: number
  rudderCentreToStop: number
  sheetHaul: number
  sheetEase: number
  sheetRelease: number
}

/**
 * How long a full-scale command takes to run an actuator end to end.
 *
 * `(max − min) / rate` — the travel divided by the rate the core moves it at,
 * both read **live** from the catalogue (task 9.2). It is a reference time for
 * the player and for `tests/e2e/mobile-controls.spec.ts`, it feeds no command,
 * and it restates nothing: every number in it is one the core published a
 * moment ago, so a live brief §31 edit moves it.
 *
 * It is not a settling time and it is not a hand-stroke time. A P controller
 * reaching a setpoint and a sailor's arm taking up rope are different
 * quantities, and neither is this one; the 2.4 s haul and 0.6 s release of the
 * superseded proposal are not targets (`discussions/mobile-controls.md`).
 *
 * The rudder's is **stop to stop**, `2·delta_r_max / delta_r_rate_max`,
 * because that is the travel a player feels when reversing the helm; the
 * centre-to-stop half is reported beside it.
 */
export function fullTravel(params: RenderParams): FullTravel {
  const sheetSpan = params.sheet.l_sheet_max - params.sheet.l_sheet_min
  return {
    rudderStopToStop: seconds(2 * params.rudder.delta_r_max, params.rudder.delta_r_rate_max),
    rudderCentreToStop: seconds(params.rudder.delta_r_max, params.rudder.delta_r_rate_max),
    sheetHaul: seconds(sheetSpan, params.sheet.sheet_haul_rate),
    sheetEase: seconds(sheetSpan, params.sheet.sheet_ease_rate),
    sheetRelease: seconds(sheetSpan, params.sheet.sheet_release_rate),
  }
}

/** `travel / rate`, guarded against a zero or absent rate. */
function seconds(travel: number, rate: number): number {
  return rate > 0 ? travel / rate : Number.POSITIVE_INFINITY
}

function signedCommand(value: number | null): string {
  if (value === null) {
    return 'idle'
  }
  return `${value >= 0 ? '+' : '−'}${Math.abs(value).toFixed(2)}`
}

function describeCommand(value: number | null, negative: string, positive: string): string {
  if (value === null || value === 0) {
    return 'no command'
  }
  return `${Math.abs(value).toFixed(2)} ${value > 0 ? positive : negative}`
}
