import { useEffect, useRef } from 'react'

import type { ReplaySource } from '../sim/replay'

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
}: TimelineProps) {
  const { startTime, endTime, frameCount } = source
  const index = source.indexAt(time)

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
    onTime(source.frameAt(i).t)
  }

  return (
    <div
      data-testid="timeline"
      data-frames={frameCount}
      data-index={index}
      data-time={time}
      data-start={startTime}
      data-end={endTime}
      data-playing={playing ? 'true' : 'false'}
      data-speed={speed}
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
        disabled={index <= 0}
      >
        ◀ Frame
      </button>
      <button
        type="button"
        data-testid="timeline-step-forward"
        onClick={() => goToFrame(index + 1)}
        disabled={index >= frameCount - 1}
      >
        Frame ▶
      </button>
      <button
        type="button"
        data-testid="timeline-reset"
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
      <span data-testid="timeline-readout">
        frame {index + 1}/{frameCount} · t = {time.toFixed(2)} s
      </span>
      <button type="button" data-testid="timeline-exit" onClick={onExit}>
        Back to live
      </button>
      {/* Said plainly rather than left to be discovered: an episode frame
          carries the F3 state, the four component forces, the four moments,
          the sheet tension and the wind — not the whole fifty-field brief
          §30 record — so the debug panel keeps describing the (paused) live
          simulation while the world view replays. Section 09's handoff
          records this as the one thing an episode inspector will want next. */}
      <span data-testid="timeline-note" style={{ color: '#667' }}>
        the world view follows the episode; the diagnostics panel shows the live, paused
        simulation
      </span>
    </div>
  )
}
