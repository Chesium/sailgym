import { useState } from 'react'

import { CompareChart, type CompareChartData } from './Charts'
import type { PracticeAttempt } from './store'
import type { ComparabilityVerdict } from '../sim/episodeIo'
import type {
  PracticeChallenge,
  PracticeEvent,
  PracticeReport,
  PracticeState,
} from '../sim/scenarioTypes'
import { radiansToDegrees } from '../sim/units'

/**
 * Guided practice: choose a goal, sail, see the result, inspect the moment,
 * retry under the same conditions (v2 section 11, task 11.3).
 *
 * ## Everything here is formatting
 *
 * Not one number on this panel is computed here. The outcome, the elapsed
 * time, the step count, the metric, the progress and every event come from
 * `sailgym_task` through `Sim.practice_state_json`, decided on a physics step
 * (v2 F18.4); the two-attempt deltas are subtractions of those numbers; the
 * comparison verdict is `ExperimentIdentity::compare`'s, in Rust (F18.3); and
 * the comparison plot is built from **recorded episode samples**. There is no
 * threshold, no rule and no scoring on this side of the boundary (F8, RV61).
 *
 * ## What the words may say
 *
 * A result explains itself with the **event that was observed** and the value
 * that was recorded — "the heel reached 75.0° with the sheet still in" — and
 * never with a cause nobody measured. The standing note at the foot of the
 * panel says what the simulation is; no text here claims that finishing a
 * challenge means anything about a real boat.
 *
 * ## Layout, and why the chooser is three lines rather than three cards
 *
 * One column that wraps, with nothing wider than its container, so the whole
 * panel works inside the 360 px world of `ui/Layout.tsx`'s compact
 * arrangement. Every control is a native `<button>`, so tab order,
 * `Enter`/`Space` activation and the focus ring are the platform's.
 *
 * The panel sits in the layout's `instruments` slot, which is **below** the
 * world view and below the fold on a short desktop window — as the recording
 * controls, the heel indicator and the timeline already are. Every row it
 * adds is therefore a row of scrolling between the player and the boat, so
 * the chooser shows three one-line selectors and the instruction and goal of
 * the **one** that is selected, rather than three cards. That is also exactly
 * what the section PRD asks for: *one* short instruction and *one* measurable
 * goal before starting.
 */

/**
 * What each challenge asks a player to *do*, in words.
 *
 * **Provenance.** Presentation only. Each names the controls the player has
 * (`ui/TouchControls.tsx` and `sim/keymap.ts`) and the technique the
 * challenge teaches; the measurable goal beside it is built from the core's
 * own thresholds by {@link goalText} and is not written here. Nothing in this
 * table reaches the physics, and none of it is a rule the evaluator applies.
 */
const LESSONS: Record<string, { title: string; instruction: string }> = {
  get_moving: {
    title: 'Get moving',
    instruction:
      'The mainsheet is right out and the boat is going nowhere. Sheet in — drag down on the boat, or down on the sheet pad — until the sail is pulling, then leave it there. Too far in is as slow as too far out.',
  },
  complete_tack: {
    title: 'Complete a tack',
    instruction:
      'You are close-hauled with the wind on the port bow. Steer up into it with A (or the helm pad) and keep the sheet hauled in as the bow swings, so the sail is still driving when you pass through the wind. Let go of both and you will stop head to wind.',
  },
  recover_from_heel: {
    title: 'Recover from excessive heel',
    instruction:
      'The sheet is hard in and the boat is lying down. Ease or release it — Space, or the release button — and get the heel back under control. This boat does not come back up once it is over, so the sooner you let the sail go, the less far it goes.',
  },
}

/** The label a metric id is shown under, and whether it reads in degrees. */
const METRICS: Record<string, { label: string; display: 'si' | 'deg'; better: 'lower' | 'higher' }> =
  {
    top_speed: { label: 'Top speed', display: 'si', better: 'higher' },
    tack_time: { label: 'Time to complete the tack', display: 'si', better: 'lower' },
    peak_heel: { label: 'Peak heel', display: 'deg', better: 'lower' },
  }

/** What each event means, for the result's own account of what happened. */
const EVENTS: Record<string, { label: string; display: 'si' | 'deg' | 'none'; unit: string }> = {
  speed_reached: { label: 'reached the target speed', display: 'si', unit: 'm/s' },
  hold_broken: { label: 'dropped back below it', display: 'si', unit: 'm/s' },
  approach: { label: 'luffed up towards the wind', display: 'deg', unit: '°' },
  crossing: { label: 'crossed head to wind', display: 'deg', unit: '°' },
  settled: { label: 'settled on the new tack', display: 'deg', unit: '°' },
  reversal: { label: 'fell back to the old tack', display: 'deg', unit: '°' },
  bore_away: { label: 'turned away from the wind instead', display: 'deg', unit: '°' },
  heel_qualified: { label: 'started to heel', display: 'deg', unit: '°' },
  heel_max: { label: 'heeled further', display: 'deg', unit: '°' },
  release: { label: 'eased the sheet', display: 'deg', unit: '°' },
  heel_recovered: { label: 'came back upright', display: 'deg', unit: '°' },
  succeeded: { label: 'finished the challenge', display: 'none', unit: '' },
  timed_out: { label: 'ran out of time', display: 'none', unit: '' },
  failed_backward_drift: { label: 'was making sternway', display: 'si', unit: 'm/s' },
  failed_wrong_way: { label: 'had turned away from the wind, not through it', display: 'none', unit: '' },
  failed_repeated_jitter: { label: 'crossed the boundary back and forth too often', display: 'none', unit: '' },
  failed_late_release: { label: 'still had the sheet in', display: 'deg', unit: '°' },
  failed_capsized: { label: 'capsized', display: 'deg', unit: '°' },
}

/** `1.23 m/s`, or `70.3°` for a value the core reports in radians. */
function show(value: number, display: 'si' | 'deg' | 'none', unit: string): string {
  if (display === 'none') {
    return ''
  }
  const v = display === 'deg' ? radiansToDegrees(value) : value
  return `${v.toFixed(display === 'deg' ? 1 : 2)} ${unit}`.trim()
}

/** One event, in a sentence: what happened, when, and at what value. */
export function eventLine(event: PracticeEvent): string {
  const meta = EVENTS[event.id] ?? { label: event.id, display: 'si' as const, unit: '' }
  const value = show(event.value, meta.display, meta.unit)
  const at = `at ${event.t.toFixed(2)} s`
  return value === '' ? `${meta.label} ${at}` : `${meta.label} ${at} — ${value}`
}

/**
 * The measurable goal, built from the challenge's own thresholds.
 *
 * Every number in the returned sentence came across the boundary in
 * `practice_tasks_json`; none of them is written on this side (F8, RV61). A
 * challenge whose id this does not know still gets its thresholds listed, so
 * a new one is never silently unexplained.
 */
export function goalText(challenge: PracticeChallenge): string {
  const t = challenge.thresholds
  const deg = (key: string) => radiansToDegrees(t[key] ?? 0).toFixed(0)
  switch (challenge.id) {
    case 'get_moving':
      return `Hold ${t.target_speed_mps} m/s of forward speed for ${t.hold_s} s, within ${t.time_limit_s} s.`
    case 'complete_tack':
      return `Cross head to wind and settle more than ${deg('settled_twa_rad')}° onto the other tack for ${t.settle_hold_s} s, still making at least ${t.recover_speed_mps} m/s, within ${t.time_limit_s} s. Turning more than ${deg('wrong_way_twa_rad')}° away from the wind ends the attempt.`
    case 'recover_from_heel':
      return `Get the heel back under ${deg('recover_heel_rad')}° and hold it there for ${t.recover_hold_s} s, within ${t.time_limit_s} s. Reaching ${deg('late_release_heel_rad')}° with the sheet still in ends the attempt, and so does a capsize.`
    default:
      return Object.entries(t)
        .map(([k, v]) => `${k} = ${v}`)
        .join(' · ')
  }
}

/** `Succeeded`, `Timed out`, `Failed — the boat capsized`. */
function outcomeLine(report: PracticeReport): string {
  switch (report.outcome.kind) {
    case 'succeeded':
      return 'Done'
    case 'timed_out':
      return 'Out of time'
    case 'failed':
      return 'Attempt over'
    default:
      return 'Sailing'
  }
}

/** The metric, formatted in the unit the panel shows it in. */
function metricText(report: PracticeReport): { label: string; text: string } {
  const meta = METRICS[report.metric.id] ?? {
    label: report.metric.id,
    display: 'si' as const,
    better: 'higher' as const,
  }
  const display = meta.display === 'deg' ? 'deg' : 'si'
  const unit = meta.display === 'deg' ? '°' : report.metric.unit
  return { label: meta.label, text: show(report.metric.value, display, unit) }
}

/** The metric value as the panel displays it — degrees where it shows degrees. */
export function displayMetric(report: PracticeReport): number {
  const meta = METRICS[report.metric.id]
  return meta?.display === 'deg' ? radiansToDegrees(report.metric.value) : report.metric.value
}

const CARD: React.CSSProperties = {
  border: '1px solid #cbd',
  borderRadius: 4,
  padding: 8,
  minWidth: 0,
}

const MUTED: React.CSSProperties = { color: '#667', fontSize: 12 }

export interface PracticePanelProps {
  ready: boolean
  /** The three shipped challenges, as the core lists them. */
  challenges: readonly PracticeChallenge[]
  /** The attempt in progress, from `Sim.practice_state_json`. */
  state: PracticeState
  /** At most the two most recent finished attempts, newest last. */
  attempts: readonly PracticeAttempt[]
  /** The core's verdict on comparing them, or `null` when there are not two. */
  comparison: ComparabilityVerdict | null
  /** The two attempts' recorded traces on one axis, or `null`. */
  compare: CompareChartData | null
  onStart: (id: string) => void
  onRetry: () => void
  onCancel: () => void
  onInspect: (attempt: PracticeAttempt) => void
  /** A recorded episode is open; **Inspect** would be a no-op. */
  replaying: boolean
}

export function PracticePanel({
  ready,
  challenges,
  state,
  attempts,
  comparison,
  compare,
  onStart,
  onRetry,
  onCancel,
  onInspect,
  replaying,
}: PracticePanelProps) {
  const report = state.active ? state.report : null
  const status = state.active ? state.status : 'none'
  const running = state.active && state.status === 'active'
  const challenge =
    report === null ? null : (challenges.find((c) => c.id === report.task.id) ?? null)
  const phase = report === null ? 'choose' : running ? 'sailing' : 'result'

  return (
    <section
      data-testid="practice"
      data-phase={phase}
      data-ready={ready ? 'true' : 'false'}
      data-task={report?.task.id ?? ''}
      data-status={status}
      data-attempts={attempts.length}
      aria-label="Guided practice"
      style={{ ...CARD, display: 'grid', gap: 8, flex: '1 1 360px' }}
    >
      <div style={{ display: 'flex', gap: 8, alignItems: 'baseline', flexWrap: 'wrap' }}>
        <strong style={{ flex: '1 1 auto' }}>Practice</strong>
        {phase !== 'choose' && (
          <button type="button" data-testid="practice-free-sail" onClick={onCancel}>
            Free sail
          </button>
        )}
      </div>

      {phase === 'choose' && (
        <Chooser ready={ready} challenges={challenges} onStart={onStart} />
      )}

      {phase === 'sailing' && report !== null && (
        <Sailing report={report} challenge={challenge} onCancel={onCancel} />
      )}

      {phase === 'result' && report !== null && (
        <Result
          report={report}
          challenge={challenge}
          status={status}
          replaying={replaying}
          onRetry={onRetry}
        />
      )}

      {attempts.length > 0 && (
        <Attempts
          attempts={attempts}
          comparison={comparison}
          compare={compare}
          replaying={replaying}
          onInspect={onInspect}
        />
      )}

      <p data-testid="practice-disclaimer" style={{ ...MUTED, margin: 0 }}>
        sailgym is a reduced model for seeing cause and effect, not sailing instruction. What the
        boat does here is what these equations do.
      </p>
    </section>
  )
}

function Chooser({
  ready,
  challenges,
  onStart,
}: {
  ready: boolean
  challenges: readonly PracticeChallenge[]
  onStart: (id: string) => void
}) {
  const [selected, setSelected] = useState<string | null>(null)
  const current = challenges.find((c) => c.id === selected) ?? challenges[0] ?? null
  const lesson = current === null ? null : (LESSONS[current.id] ?? { title: current.id, instruction: '' })

  return (
    <div style={{ display: 'grid', gap: 6 }}>
      <p style={{ ...MUTED, margin: 0 }}>
        Pick a goal, sail it, then look at what happened and try again under the same conditions.
        Or just keep sailing — nothing here has to be started.
      </p>
      <div role="radiogroup" aria-label="Challenge" style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
        {challenges.map((c) => (
          <button
            key={c.id}
            type="button"
            role="radio"
            aria-checked={current?.id === c.id}
            data-testid={`practice-challenge-${c.id}`}
            data-scenario={c.scenario}
            data-version={c.version}
            data-selected={current?.id === c.id ? 'true' : 'false'}
            onClick={() => setSelected(c.id)}
            style={{ fontWeight: current?.id === c.id ? 700 : 400 }}
          >
            {LESSONS[c.id]?.title ?? c.id}
          </button>
        ))}
      </div>
      {current !== null && lesson !== null && (
        <div style={{ display: 'grid', gap: 4 }}>
          <p style={{ margin: 0 }}>{lesson.instruction}</p>
          <p data-testid={`practice-goal-${current.id}`} style={{ ...MUTED, margin: 0 }}>
            Goal: {goalText(current)}
          </p>
          <div style={{ display: 'flex', gap: 8, alignItems: 'baseline', flexWrap: 'wrap' }}>
            <button
              type="button"
              data-testid={`practice-start-${current.id}`}
              disabled={!ready}
              onClick={() => onStart(current.id)}
            >
              Start
            </button>
            <span style={MUTED}>
              scenario {current.scenario} · task version {current.version}
            </span>
          </div>
        </div>
      )}
    </div>
  )
}

function Sailing({
  report,
  challenge,
  onCancel,
}: {
  report: PracticeReport
  challenge: PracticeChallenge | null
  onCancel: () => void
}) {
  const lesson = LESSONS[report.task.id] ?? { title: report.task.id, instruction: '' }
  const p = report.progress
  const holdFraction =
    p.hold_target_s > 0 ? Math.min(1, Math.max(0, p.hold_s / p.hold_target_s)) : 0
  const limit = challenge?.time_limit_s ?? 0
  return (
    <div style={{ display: 'grid', gap: 6 }}>
      <div style={{ display: 'flex', gap: 8, alignItems: 'baseline', flexWrap: 'wrap' }}>
        <strong style={{ flex: '1 1 auto' }}>{lesson.title}</strong>
        <button type="button" data-testid="practice-cancel" onClick={onCancel}>
          Give up
        </button>
      </div>
      <p style={{ ...MUTED, margin: 0 }}>{challenge === null ? '' : goalText(challenge)}</p>
      <div
        data-testid="practice-progress"
        data-phase={p.phase}
        data-elapsed={report.elapsed_s}
        data-steps={report.elapsed_steps}
        data-value={p.value}
        data-target={p.target}
        data-hold={p.hold_s}
        data-hold-target={p.hold_target_s}
        aria-live="polite"
        style={{ display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' }}
      >
        <span style={{ fontVariantNumeric: 'tabular-nums' }}>
          {report.elapsed_s.toFixed(1)} s{limit > 0 && ` of ${limit.toFixed(0)} s`}
        </span>
        {p.hold_target_s > 0 && (
          <span
            aria-label="hold progress"
            style={{
              flex: '1 1 80px',
              minWidth: 60,
              height: 6,
              borderRadius: 3,
              background: '#dde3ea',
              overflow: 'hidden',
            }}
          >
            <span
              style={{
                display: 'block',
                width: `${(holdFraction * 100).toFixed(1)}%`,
                height: '100%',
                background: '#2b8a3e',
              }}
            />
          </span>
        )}
        <span style={MUTED}>{p.phase}</span>
      </div>
    </div>
  )
}

function Result({
  report,
  challenge,
  status,
  replaying,
  onRetry,
}: {
  report: PracticeReport
  challenge: PracticeChallenge | null
  status: string
  replaying: boolean
  onRetry: () => void
}) {
  const lesson = LESSONS[report.task.id] ?? { title: report.task.id, instruction: '' }
  const metric = metricText(report)
  const terminal = report.events.length === 0 ? null : report.events[report.events.length - 1]
  const succeeded = report.outcome.kind === 'succeeded'
  return (
    <div
      data-testid="practice-result"
      data-outcome={report.outcome.kind}
      data-reason={report.outcome.reason ?? ''}
      data-status={status}
      data-metric-id={report.metric.id}
      data-metric={report.metric.value}
      data-elapsed={report.elapsed_s}
      data-steps={report.elapsed_steps}
      data-events={report.events.length}
      aria-live="polite"
      style={{ display: 'grid', gap: 6 }}
    >
      <div style={{ display: 'flex', gap: 8, alignItems: 'baseline', flexWrap: 'wrap' }}>
        <strong style={{ flex: '1 1 auto', color: succeeded ? '#2b8a3e' : '#a05000' }}>
          {lesson.title} — {outcomeLine(report)}
        </strong>
        <button type="button" data-testid="practice-retry" onClick={onRetry}>
          Retry
        </button>
      </div>

      {status === 'conditions_changed' && (
        <p data-testid="practice-conditions-changed" style={{ margin: 0, color: '#a05000' }}>
          A parameter or the wind was changed while this attempt was running, so the conditions it
          started under no longer held. It cannot be compared with another attempt.
        </p>
      )}
      {status === 'cancelled' && (
        <p style={{ margin: 0, color: '#a05000' }}>
          The run was reset while this attempt was in progress, so there is no result.
        </p>
      )}

      <p style={{ margin: 0, fontVariantNumeric: 'tabular-nums' }}>
        {metric.label}: <strong>{metric.text}</strong> · {report.elapsed_s.toFixed(2)} s of task
        time · {report.elapsed_steps} physics steps
      </p>

      {terminal !== null && (
        <p data-testid="practice-why" data-event={terminal.id} style={{ margin: 0 }}>
          What was recorded: the boat {eventLine(terminal)}.
          {report.outcome.reason === 'capsized' &&
            ' This simulation does not right a boat that has gone over.'}
        </p>
      )}

      <details>
        <summary style={MUTED}>everything that was recorded ({report.events.length})</summary>
        <ol
          data-testid="practice-events"
          style={{ margin: 0, paddingLeft: 18, ...MUTED, maxHeight: 120, overflowY: 'auto' }}
        >
          {report.events.map((e, i) => (
            <li
              key={`${e.id}-${e.step}-${i}`}
              data-event-id={e.id}
              data-step={e.step}
              data-t={e.t}
              data-value={e.value}
            >
              {eventLine(e)}
            </li>
          ))}
        </ol>
      </details>

      {challenge !== null && (
        <p style={{ ...MUTED, margin: 0 }}>
          Goal was: {goalText(challenge)} (task version {challenge.version})
        </p>
      )}
      {replaying && (
        <p style={{ ...MUTED, margin: 0 }}>
          A recorded episode is open — use the timeline below, then Retry when you are ready.
        </p>
      )}
    </div>
  )
}

function Attempts({
  attempts,
  comparison,
  compare,
  replaying,
  onInspect,
}: {
  attempts: readonly PracticeAttempt[]
  comparison: ComparabilityVerdict | null
  compare: CompareChartData | null
  replaying: boolean
  onInspect: (attempt: PracticeAttempt) => void
}) {
  const comparable = comparison?.verdict === 'same_conditions'
  const [older, newer] = attempts.length === 2 ? attempts : [null, attempts[0] ?? null]
  const deltaMetric =
    comparable && older !== null && newer !== null
      ? displayMetric(newer.report) - displayMetric(older.report)
      : null
  const deltaElapsed =
    comparable && older !== null && newer !== null
      ? newer.report.elapsed_s - older.report.elapsed_s
      : null
  const unit =
    METRICS[newer?.report.metric.id ?? '']?.display === 'deg'
      ? '°'
      : (newer?.report.metric.unit ?? '')

  return (
    <div
      data-testid="practice-compare"
      data-count={attempts.length}
      data-verdict={comparison?.verdict ?? ''}
      data-comparable={comparable ? 'true' : 'false'}
      data-delta-metric={deltaMetric ?? ''}
      data-delta-elapsed={deltaElapsed ?? ''}
      style={{ ...CARD, display: 'grid', gap: 6 }}
    >
      <strong>Attempts kept ({attempts.length} of 2)</strong>
      <ul style={{ margin: 0, paddingLeft: 0, listStyle: 'none', display: 'grid', gap: 4 }}>
        {attempts.map((a) => {
          const m = metricText(a.report)
          return (
            <li
              key={a.id}
              data-testid={`practice-attempt-${a.id}`}
              data-task={a.taskId}
              data-outcome={a.report.outcome.kind}
              data-metric={a.report.metric.value}
              style={{ display: 'flex', gap: 8, alignItems: 'baseline', flexWrap: 'wrap' }}
            >
              <span style={{ flex: '1 1 auto', fontVariantNumeric: 'tabular-nums' }}>
                {LESSONS[a.taskId]?.title ?? a.taskId} · {a.report.outcome.kind.replace('_', ' ')} ·{' '}
                {m.label} {m.text} · {a.report.elapsed_s.toFixed(2)} s
              </span>
              {/* Enabled whenever a replay is not already open. An attempt
                  with no highlight event — a challenge that timed out before
                  anything happened — still has an episode worth looking at,
                  and `App.tsx` opens it at the start rather than pretending
                  there is a moment to jump to. */}
              <button
                type="button"
                data-testid={`practice-inspect-${a.id}`}
                data-highlight={a.report.highlight?.id ?? ''}
                disabled={replaying}
                onClick={() => onInspect(a)}
              >
                Inspect
              </button>
            </li>
          )
        })}
      </ul>

      {attempts.length === 2 && (
        <p data-testid="practice-compare-verdict" style={{ margin: 0 }}>
          {comparable ? (
            <>
              Same conditions, so these two are comparable.{' '}
              {deltaMetric !== null && (
                <span data-testid="practice-delta">
                  {METRICS[newer?.report.metric.id ?? '']?.label ?? 'metric'}{' '}
                  {deltaMetric >= 0 ? '+' : '−'}
                  {Math.abs(deltaMetric).toFixed(2)} {unit}
                  {deltaElapsed !== null && (
                    <>
                      , task time {deltaElapsed >= 0 ? '+' : '−'}
                      {Math.abs(deltaElapsed).toFixed(2)} s
                    </>
                  )}
                  {' '}on the newer attempt.
                </span>
              )}
            </>
          ) : (
            <>
              These two attempts are <strong>not</strong> comparable
              {comparison === null ? '' : `: ${comparison.describe}`}. Each can still be inspected
              on its own.
            </>
          )}
        </p>
      )}

      {compare !== null && comparable && <CompareChart data={compare} />}
    </div>
  )
}
