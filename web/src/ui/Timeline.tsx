import { useEffect, useRef } from 'react'

import { NOT_RECORDED } from '../sim/diagnostics'
import type { ReplaySource } from '../sim/replay'
import type { PracticeEvent } from '../sim/scenarioTypes'

/**
 * The replay transport (brief §33, task 9.4): play, pause, scrub, step,
 * playback speed and reset to episode start.
 *
 * **It drives nothing but an index.** The frames come from the recorder and
 * the renderer reads them exactly as it reads a live snapshot; no physics is
 * recomputed and the `Sim` is not touched, which is why replay works with the
 * physics clock paused.
 *
 * The animation frame loop below is the one exception to "the application has
 * a single rAF loop": it exists only while a replay is actually *playing*,
 * and it advances a number. The physics loop in `sim/useSimulation.ts`
 * remains the only one that can reach the core.
 *
 * ## Degenerate episodes have defined behaviour (v2 section 10, task 10.4)
 *
 * {@link timelineState} is the pure rule, and it is what the unit tests
 * exercise:
 *
 * | frames | scrub | step | play |
 * |---|---|---|---|
 * | 0 | disabled | disabled | disabled |
 * | 1 | disabled — the span is a point | disabled | disabled |
 * | ≥ 2 | enabled over `[startTime, endTime]` | bounded | enabled |
 *
 * A one-frame episode is a **constant** timeline, not a broken one: the
 * playhead is pinned to that sample's own time and the transport says so.
 * Scrubbing before the start or past the end clamps to the ends rather than
 * extrapolating — a replay has no data outside its own episode.
 */

/** Playback speeds, in display order. `1` is the recorded rate. */
export const PLAYBACK_SPEEDS = [0.25, 0.5, 1, 2, 4] as const
export type PlaybackSpeed = (typeof PLAYBACK_SPEEDS)[number]

export interface TimelineProps {
  source: ReplaySource
  /** Current playhead, in the episode's own simulated seconds. */
  time: number
  playing: boolean
  speed: PlaybackSpeed
  onTime: (t: number) => void
  onPlaying: (playing: boolean) => void
  onSpeed: (speed: PlaybackSpeed) => void
  /** Leave replay and go back to the live simulation. */
  onExit: () => void
  /** How many `Diagnostics` fields this episode does not carry. */
  unavailable: number
  /** Why the spatial wind field is not drawn. */
  windFieldReason: string
  /**
   * The episode's recorded practice events, if it carries any (v2 section 11).
   *
   * Each becomes a button that puts the playhead on the event's **own**
   * recorded time. The event was decided on a physics step and carries that
   * step's `t` (v2 F18.4), so landing on it lands on a moment that happened —
   * unlike a scrub, which may sit between two recorded samples.
   */
  events?: readonly PracticeEvent[]
}

/** What the transport may do with a given episode, and where the playhead is. */
export interface TimelineState {
  frames: number
  /** The playhead, clamped into the episode. */
  time: number
  /** The recorded sample at or before {@link TimelineState.time}; `-1` if none. */
  index: number
  startTime: number
  endTime: number
  /** Simulated seconds the episode spans; `0` for 0 or 1 frames. */
  span: number
  /** Fewer than two samples: there is nothing to scrub through. */
  scrubDisabled: boolean
  canStepBack: boolean
  canStepForward: boolean
  /** Fewer than two samples: playing would advance through nothing. */
  playDisabled: boolean
  /** One line describing the episode's extent, for the readout. */
  label: string
}

/**
 * The transport's rule, as a pure function.
 *
 * Separated from the component because it is the part with cases — empty, one
 * frame, many — and because `tests/unit/replay.test.ts` runs in Node with no
 * DOM, so this is the half that can be asserted directly.
 */
export function timelineState(source: ReplaySource, time: number): TimelineState {
  const { frameCount, startTime, endTime } = source
  const degenerate = frameCount < 2
  // Clamping rather than extrapolating: a replay has no data outside its own
  // episode, in either direction.
  const clamped =
    frameCount === 0 ? 0 : Math.min(endTime, Math.max(startTime, Number.isFinite(time) ? time : startTime))
  const index = source.indexAt(clamped)
  return {
    frames: frameCount,
    time: clamped,
    index,
    startTime,
    endTime,
    span: degenerate ? 0 : endTime - startTime,
    scrubDisabled: degenerate,
    canStepBack: !degenerate && index > 0,
    canStepForward: !degenerate && index < frameCount - 1,
    playDisabled: degenerate,
    label:
      frameCount === 0
        ? 'no samples'
        : frameCount === 1
          ? `one sample at t = ${startTime.toFixed(2)} s`
          : `frame ${index + 1}/${frameCount} · t = ${clamped.toFixed(2)} s`,
  }
}

export function Timeline({
  source,
  time,
  playing,
  speed,
  onTime,
  onPlaying,
  onSpeed,
  onExit,
  unavailable,
  windFieldReason,
  events = [],
}: TimelineProps) {
  const { startTime, endTime, frameCount, logHz, schemaVersion } = source
  const state = timelineState(source, time)
  const index = state.index

  // Held in refs so the loop below is started by `playing` alone and not
  // restarted on every playhead update.
  const timeRef = useRef(time)
  timeRef.current = time
  const speedRef = useRef(speed)
  speedRef.current = speed
  const onTimeRef = useRef(onTime)
  onTimeRef.current = onTime
  const onPlayingRef = useRef(onPlaying)
  onPlayingRef.current = onPlaying

  useEffect(() => {
    if (!playing) {
      return
    }
    let frame = 0
    let previous: number | null = null
    const tick = (now: number) => {
      frame = requestAnimationFrame(tick)
      const delta = previous === null ? 0 : (now - previous) / 1000
      previous = now
      const next = timeRef.current + delta * speedRef.current
      if (next >= endTime) {
        onTimeRef.current(endTime)
        onPlayingRef.current(false)
        return
      }
      onTimeRef.current(next)
    }
    frame = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(frame)
  }, [playing, endTime])

  const goToFrame = (i: number) => {
    onPlaying(false)
    const frame = source.frameAt(i)
    if (frame !== null) {
      onTime(frame.t)
    }
  }

  return (
    <div
      data-testid="timeline"
      data-frames={frameCount}
      data-index={index}
      data-time={time}
      data-start={startTime}
      data-end={endTime}
      data-span={state.span}
      data-playing={playing ? 'true' : 'false'}
      data-speed={speed}
      // The episode's own sampling resolution, its schema, and how much of the
      // diagnostics record it does not carry. All three change what a viewer
      // may conclude from what is on screen, so all three are on the page.
      data-log-hz={logHz}
      data-schema={schemaVersion}
      data-unavailable={unavailable}
      data-degenerate={state.scrubDisabled ? 'true' : 'false'}
      style={{
        display: 'flex',
        gap: 6,
        alignItems: 'center',
        flexWrap: 'wrap',
        padding: '4px 8px',
        border: '1px solid #cbd',
        borderRadius: 4,
        background: '#f6f8fb',
      }}
    >
      <strong>Replay</strong>
      <button
        type="button"
        data-testid="timeline-play"
        data-playing={playing ? 'true' : 'false'}
        disabled={state.playDisabled}
        onClick={() => {
          // Pressing play at the very end starts again from the beginning,
          // rather than doing nothing.
          if (!playing && time >= endTime) {
            onTime(startTime)
          }
          onPlaying(!playing)
        }}
      >
        {playing ? 'Pause' : 'Play'}
      </button>
      <button
        type="button"
        data-testid="timeline-step-back"
        onClick={() => goToFrame(index - 1)}
        disabled={!state.canStepBack}
      >
        ◀ Frame
      </button>
      <button
        type="button"
        data-testid="timeline-step-forward"
        onClick={() => goToFrame(index + 1)}
        disabled={!state.canStepForward}
      >
        Frame ▶
      </button>
      <button
        type="button"
        data-testid="timeline-reset"
        disabled={frameCount === 0}
        onClick={() => {
          onPlaying(false)
          onTime(startTime)
        }}
        title="Reset to episode start"
      >
        ⏮ Start
      </button>
      <input
        type="range"
        data-testid="timeline-scrub"
        min={startTime}
        max={endTime}
        // Continuous: the playhead is not restricted to the logged sample
        // times, which is exactly what `sampleAt`'s interpolation is for.
        // The two frame buttons are how you land *on* a sample.
        step="any"
        value={time}
        disabled={state.scrubDisabled}
        onChange={(e) => {
          onPlaying(false)
          onTime(Number(e.target.value))
        }}
        style={{ flex: '1 1 220px', minWidth: 160 }}
      />
      {PLAYBACK_SPEEDS.map((s) => (
        <button
          key={s}
          type="button"
          data-testid={`timeline-speed-${s}x`}
          data-active={speed === s}
          onClick={() => onSpeed(s)}
          style={{ fontWeight: speed === s ? 700 : 400 }}
        >
          {s}×
        </button>
      ))}
      <span data-testid="timeline-readout">{state.label}</span>
      <span data-testid="timeline-resolution" style={{ color: '#667' }}>
        sampled at {logHz} Hz · schema {schemaVersion}
      </span>
      <button type="button" data-testid="timeline-exit" onClick={onExit}>
        Back to live
      </button>
      {events.length > 0 && (
        <span
          data-testid="timeline-events"
          data-count={events.length}
          style={{ display: 'flex', gap: 4, flexWrap: 'wrap', alignItems: 'center' }}
        >
          <span style={{ color: '#667' }}>jump to:</span>
          {events.map((e, i) => (
            <button
              key={`${e.id}-${e.step}-${i}`}
              type="button"
              data-testid={`timeline-event-${e.id}`}
              data-step={e.step}
              data-t={e.t}
              title={`${e.id} at step ${e.step}, t = ${e.t.toFixed(3)} s`}
              onClick={() => {
                onPlaying(false)
                onTime(e.t)
              }}
            >
              {e.id}
            </button>
          ))}
        </span>
      )}
      {/* v2 section 10 replaced what this note used to say. Every panel now
          reads the episode: the HUD, the force overlay, the charts and the
          diagnostics panel all take the recorded sample, and what the episode
          does not carry says so instead of borrowing from the live run. */}
      <span data-testid="timeline-note" style={{ color: '#667' }}>
        every panel shows this episode
        {unavailable > 0 && (
          <> · {unavailable} diagnostics {NOT_RECORDED.toLowerCase()}</>
        )}
        {windFieldReason !== '' && <> · wind field hidden</>}
      </span>
    </div>
  )
}
