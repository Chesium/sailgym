/**
 * Replay of a recorded episode (brief §33, task 9.4).
 *
 * **A replay consumes stored trajectory data; it does not recompute the
 * physics.** Nothing in this file touches the `Sim`: it indexes frames the
 * recorder produced, and the renderer reads a replay frame exactly as it
 * reads a live snapshot, so the same components work in both modes.
 *
 * `replay.spec.ts` proves the "does not recompute" claim the only way it can
 * be proved — by editing one frame in memory and watching the render follow
 * the edit.
 *
 * ## What the interpolation is, and is not
 *
 * {@link ReplaySource.sampleAt} exists so scrubbing looks smooth between the
 * logged samples. It is **display only** and is not physics (F8): no equation
 * of motion, no coefficient and no integration happens here, and an
 * interpolated frame is never fed back into the core. Exactly at a frame's
 * own time it returns that frame's values unchanged.
 *
 * Two kinds of field are deliberately *not* lerped:
 *
 * * `controls` and `capsized` are **commands and reports**, not continuous
 *   signals. Averaging a rudder command of −1 with one of +1 would display a
 *   centred rudder at an instant when the helm was never centred, so both are
 *   held at the earlier frame's value until the later frame's time arrives.
 * * `psi` and `beta` are wrapped to `(−π, π]` (F3), so they are interpolated
 *   along the **shorter arc**: a boat swinging through due west must not
 *   appear to spin the long way round when the recorded value steps from
 *   `+3.14` to `−3.14`. `phi` is not wrapped (F3) and is lerped plainly.
 */

import type { Episode, EpisodeFrame, EpisodeHeader } from './scenarioTypes'
import { SNAPSHOT_FIELDS, type Snapshot } from './snapshot'

/** Indices into `EpisodeFrame.state` that carry an angle wrapped to (−π, π]. */
const WRAPPED_STATE_INDICES: readonly number[] = [
  SNAPSHOT_FIELDS.indexOf('psi'),
  SNAPSHOT_FIELDS.indexOf('beta'),
]

/** Wrap an angle to `(−π, π]`. Bookkeeping for the shorter arc, not physics. */
export function wrapPi(a: number): number {
  const twoPi = 2 * Math.PI
  let x = (a + Math.PI) % twoPi
  if (x <= 0) {
    x += twoPi
  }
  return x - Math.PI
}

/** Linear interpolation, `alpha` in `[0, 1]`. */
function lerp(a: number, b: number, alpha: number): number {
  return a + (b - a) * alpha
}

/** Interpolate along the shorter arc between two wrapped angles. */
function lerpAngle(a: number, b: number, alpha: number): number {
  return wrapPi(a + wrapPi(b - a) * alpha)
}

function lerpArray(a: readonly number[], b: readonly number[], alpha: number): number[] {
  return a.map((v, i) => lerp(v, b[i], alpha))
}

/** A replayable episode. */
export interface ReplaySource {
  header: EpisodeHeader
  frameCount: number
  /** Simulated time of the first frame. */
  startTime: number
  /** Simulated time of the last frame. */
  endTime: number
  /**
   * The stored frame at `i`, clamped to the episode.
   *
   * Returns the **stored object**, not a copy: an episode inspector — and
   * `replay.spec.ts` — must be able to see that the renderer is reading these
   * frames and nothing else.
   */
  frameAt(i: number): EpisodeFrame
  /** The index of the last frame at or before `t`. */
  indexAt(t: number): number
  /** Linear interpolation between frames for smooth scrubbing. Display only. */
  sampleAt(t: number): EpisodeFrame
}

export type PlaybackMode = { kind: 'live' } | { kind: 'replay'; source: ReplaySource }

/** `{ kind: 'live' }`, as a shared constant so React sees a stable identity. */
export const LIVE: PlaybackMode = { kind: 'live' }

/**
 * Wrap a decoded episode as a replay source.
 *
 * Throws on an empty episode: there is nothing to scrub, and every caller
 * would otherwise need its own guard.
 */
export function createReplaySource(episode: Episode): ReplaySource {
  const frames = episode.frames
  if (frames.length === 0) {
    throw new Error('replay: the episode has no frames')
  }
  const first = frames[0]
  const last = frames[frames.length - 1]

  const frameAt = (i: number): EpisodeFrame => {
    const clamped = Math.min(frames.length - 1, Math.max(0, Math.trunc(i)))
    return frames[clamped]
  }

  /** The last index whose `t` is at or before `t`. Binary search. */
  const indexAt = (t: number): number => {
    if (!(t > first.t)) {
      return 0
    }
    if (t >= last.t) {
      return frames.length - 1
    }
    let lo = 0
    let hi = frames.length - 1
    while (hi - lo > 1) {
      const mid = (lo + hi) >> 1
      if (frames[mid].t <= t) {
        lo = mid
      } else {
        hi = mid
      }
    }
    return lo
  }

  const sampleAt = (t: number): EpisodeFrame => {
    const i = indexAt(t)
    const a = frames[i]
    if (i === frames.length - 1) {
      return a
    }
    const b = frames[i + 1]
    const span = b.t - a.t
    // Exactly on a frame, or two frames sharing a timestamp: the stored
    // values, unchanged.
    if (!(span > 0) || t <= a.t) {
      return a
    }
    const alpha = Math.min(1, (t - a.t) / span)
    if (alpha === 0) {
      return a
    }

    const state = lerpArray(a.state, b.state, alpha)
    for (const k of WRAPPED_STATE_INDICES) {
      state[k] = lerpAngle(a.state[k], b.state[k], alpha)
    }
    return {
      t: lerp(a.t, b.t, alpha),
      state,
      // Held, not averaged — see the note at the top of this file.
      controls: [...a.controls],
      wind_at_boat: lerpArray(a.wind_at_boat, b.wind_at_boat, alpha),
      forces: lerpArray(a.forces, b.forces, alpha),
      moments: lerpArray(a.moments, b.moments, alpha),
      sheet_tension: lerp(a.sheet_tension, b.sheet_tension, alpha),
      reward: lerp(a.reward, b.reward, alpha),
      capsized: a.capsized,
    }
  }

  return {
    header: episode.header,
    frameCount: frames.length,
    startTime: first.t,
    endTime: last.t,
    frameAt,
    indexAt,
    sampleAt,
  }
}

/**
 * A replay frame's state as the renderer's {@link Snapshot}.
 *
 * The frame carries the F3 state in F8.3 order — the same layout
 * `Sim.snapshot()` returns — so this is the index map and nothing more.
 */
export function snapshotFromFrame(frame: EpisodeFrame): Snapshot {
  const out = {} as Snapshot
  for (let i = 0; i < SNAPSHOT_FIELDS.length; i += 1) {
    out[SNAPSHOT_FIELDS[i]] = frame.state[i]
  }
  return out
}

/**
 * The replay probe the E2E suite drives (tasks 9.4, 9.5), installed on
 * `window.__sailgym` by `App.tsx`.
 *
 * It exists so `replay.spec.ts` can exercise the export/import path without
 * a real file dialog, and so it can *edit a stored frame* and watch the
 * render follow — which is the only way to prove a replay consumes stored
 * data rather than recomputing it (brief §33). Nothing in the application
 * reads it, and it is `undefined` until the app has mounted.
 */
export interface ReplayProbe {
  /** The episode currently held, as its JSON document. */
  episodeJson(): string | null
  /** The same episode in the core's binary form, as plain bytes. */
  binary(): number[] | null
  /** Import an episode from either form, through the core's own decoders. */
  load(data: number[] | string): void
  /** Simulated time of a stored frame, or `null` when not replaying. */
  frameTime(index: number): number | null
  /** A copy of a stored frame's F3 state, or `null` when not replaying. */
  frameState(index: number): number[] | null
  /** Overwrite one scalar of a stored frame, in place. */
  patchFrame(index: number, field: number, value: number): void
  /** Frames in the episode being replayed, or 0. */
  frameCount(): number
}

declare global {
  interface Window {
    __sailgym?: ReplayProbe
  }
}
