import { describe, expect, it } from 'vitest'

import {
  createReplaySource,
  snapshotFromFrame,
  wrapPi,
} from '../../src/sim/replay'
import { SNAPSHOT_FIELDS } from '../../src/sim/snapshot'
import type { Episode, EpisodeFrame, EpisodeHeader } from '../../src/sim/scenarioTypes'

/**
 * The replay player (task 9.4).
 *
 * `sampleAt` is display-only interpolation between recorded frames — it is
 * not physics (F8) and it never feeds the core. What has to be true is that
 * it is *exact* on a frame, *linear* between two, and that the two wrapped
 * angles of F3 take the short way round.
 */

const PSI = SNAPSHOT_FIELDS.indexOf('psi')
const BETA = SNAPSHOT_FIELDS.indexOf('beta')
const X = SNAPSHOT_FIELDS.indexOf('x')
const PHI = SNAPSHOT_FIELDS.indexOf('phi')

const HEADER: EpisodeHeader = {
  schema_version: 1,
  scenario: {
    schema_version: 1,
    name: 'free_sail',
    description: 'a fixture',
    seed: 0,
    parameter_overrides: {},
    initial_state: {
      x: 0,
      y: 0,
      heading_deg: 90,
      heel_deg: 0,
      speed: 0,
      boom_deg_to_port: 0,
      sheet_length: 4.5,
    },
    wind: {
      mode: 'uniform',
      speed: 5,
      bearing_deg: 0,
      variation: 0.15,
      length_scale: 120,
      time_scale: 25,
      modes: 12,
      spectral_slope: 1.5,
    },
    camera: { mode: 'northUp', zoom: 1 },
    initial_controls: null,
  },
  parameters: {},
  dt: 0.005,
  log_hz: 20,
  toolchain: { rustc: 'rustc test', target: 'test', profile: 'debug' },
  created_utc: '1970-01-01T00:00:00.000Z',
}

/** A frame with `state[i] = base + i`, so every index is distinguishable. */
function frame(t: number, overrides: Partial<Record<number, number>> = {}, base = 0): EpisodeFrame {
  const state = SNAPSHOT_FIELDS.map((_, i) => base + i)
  for (const [k, v] of Object.entries(overrides)) {
    state[Number(k)] = v as number
  }
  return {
    t,
    state,
    controls: [base, -base, base > 0 ? 1 : 0],
    wind_at_boat: [base, base + 1],
    forces: Array.from({ length: 12 }, (_, i) => base * 10 + i),
    moments: Array.from({ length: 4 }, (_, i) => base * 100 + i),
    sheet_tension: base * 3,
    reward: 0,
    capsized: base > 5,
  }
}

function episode(frames: EpisodeFrame[]): Episode {
  return { header: HEADER, frames }
}

describe('replay source', () => {
  it('returns a frame unchanged at its exact time', () => {
    const frames = [frame(0, {}, 0), frame(0.5, {}, 100), frame(1, {}, 200)]
    const source = createReplaySource(episode(frames))

    expect(source.frameCount).toBe(3)
    expect(source.startTime).toBe(0)
    expect(source.endTime).toBe(1)

    for (const f of frames) {
      const sample = source.sampleAt(f.t)
      expect(sample.t).toBe(f.t)
      expect(sample.state).toEqual(f.state)
      expect(sample.forces).toEqual(f.forces)
      expect(sample.moments).toEqual(f.moments)
      expect(sample.sheet_tension).toBe(f.sheet_tension)
      expect(sample.capsized).toBe(f.capsized)
    }

    // Before the start and after the end clamp to the ends rather than
    // extrapolating: a replay has no data outside its own episode.
    expect(source.sampleAt(-10).state).toEqual(frames[0].state)
    expect(source.sampleAt(99).state).toEqual(frames[2].state)
  })

  it('interpolates linearly between frames, against a hand-computed midpoint', () => {
    // Two frames a second apart. At t = 0.5 every lerped value must be the
    // arithmetic mean, worked out here rather than taken from the source.
    const a = frame(1, { [X]: 10, [PHI]: -0.4 }, 0)
    const b = frame(2, { [X]: 20, [PHI]: 0.6 }, 100)
    const source = createReplaySource(episode([a, b]))

    const mid = source.sampleAt(1.5)
    expect(mid.t).toBeCloseTo(1.5, 12)
    expect(mid.state[X]).toBeCloseTo(15, 12)
    // `phi` is *not* wrapped (F3), so it is a plain lerp: (−0.4 + 0.6)/2.
    expect(mid.state[PHI]).toBeCloseTo(0.1, 12)
    expect(mid.sheet_tension).toBeCloseTo((a.sheet_tension + b.sheet_tension) / 2, 12)
    for (let i = 0; i < 12; i += 1) {
      expect(mid.forces[i]).toBeCloseTo((a.forces[i] + b.forces[i]) / 2, 12)
    }
    for (let i = 0; i < 2; i += 1) {
      expect(mid.wind_at_boat[i]).toBeCloseTo((a.wind_at_boat[i] + b.wind_at_boat[i]) / 2, 12)
    }

    // A quarter of the way, too — one point could be a coincidence.
    const quarter = source.sampleAt(1.25)
    expect(quarter.state[X]).toBeCloseTo(12.5, 12)
    expect(quarter.state[PHI]).toBeCloseTo(-0.15, 12)

    // Commands and the capsize report are held, not averaged: a rudder
    // command of −1 averaged with +1 would display a helm position that
    // never happened.
    expect(mid.controls).toEqual(a.controls)
    expect(mid.capsized).toBe(a.capsized)
  })

  it('takes the short way round the ±π wrap for psi and beta', () => {
    const nearPi = Math.PI - 0.05
    const a = frame(0, { [PSI]: nearPi, [BETA]: -nearPi }, 0)
    const b = frame(1, { [PSI]: -nearPi, [BETA]: nearPi }, 0)
    const source = createReplaySource(episode([a, b]))

    // The true change is 0.1 rad across the branch cut, not 2π − 0.1.
    const trueDelta = 0.1
    let previous = source.sampleAt(0)
    let travelledPsi = 0
    let travelledBeta = 0
    for (let k = 1; k <= 100; k += 1) {
      const sample = source.sampleAt(k / 100)
      const stepPsi = Math.abs(wrapPi(sample.state[PSI] - previous.state[PSI]))
      const stepBeta = Math.abs(wrapPi(sample.state[BETA] - previous.state[BETA]))
      // No interpolated angle may jump by more than the true delta.
      expect(stepPsi).toBeLessThanOrEqual(trueDelta + 1e-9)
      expect(stepBeta).toBeLessThanOrEqual(trueDelta + 1e-9)
      travelledPsi += stepPsi
      travelledBeta += stepBeta
      // Every sample stays inside the wrapped range.
      expect(Math.abs(sample.state[PSI])).toBeLessThanOrEqual(Math.PI + 1e-12)
      expect(Math.abs(sample.state[BETA])).toBeLessThanOrEqual(Math.PI + 1e-12)
      previous = sample
    }
    // …and the total path is the short arc, not the long way round.
    expect(travelledPsi).toBeCloseTo(trueDelta, 9)
    expect(travelledBeta).toBeCloseTo(trueDelta, 9)

    // The midpoint is the wrapped ±π boundary itself.
    expect(Math.abs(source.sampleAt(0.5).state[PSI])).toBeCloseTo(Math.PI, 9)

    // A plain lerp would have gone the long way; this is what the test is
    // guarding against.
    const naive = (a.state[PSI] + b.state[PSI]) / 2
    expect(Math.abs(naive)).toBeLessThan(1e-12)
  })

  it('indexes frames and clamps out-of-range requests', () => {
    const frames = [frame(0, {}, 0), frame(0.25, {}, 10), frame(0.5, {}, 20)]
    const source = createReplaySource(episode(frames))
    expect(source.indexAt(-1)).toBe(0)
    expect(source.indexAt(0)).toBe(0)
    expect(source.indexAt(0.24)).toBe(0)
    expect(source.indexAt(0.25)).toBe(1)
    expect(source.indexAt(0.49)).toBe(1)
    expect(source.indexAt(10)).toBe(2)
    expect(source.frameAt(-5)).toBe(frames[0])
    expect(source.frameAt(99)).toBe(frames[2])
    // The stored object, not a copy: the episode inspector has to be able to
    // see that the renderer reads these very frames.
    expect(source.frameAt(1)).toBe(frames[1])
  })

  it('rejects an empty episode rather than producing an unusable player', () => {
    expect(() => createReplaySource(episode([]))).toThrow(/no frames/)
  })

  it('maps a frame onto the F8.3 snapshot layout', () => {
    const f = frame(3, {}, 0)
    const snapshot = snapshotFromFrame(f)
    SNAPSHOT_FIELDS.forEach((field, i) => {
      expect(snapshot[field]).toBe(f.state[i])
    })
  })
})

describe('wrapPi', () => {
  it('wraps to (−π, π]', () => {
    expect(wrapPi(0)).toBe(0)
    expect(wrapPi(Math.PI)).toBeCloseTo(Math.PI, 12)
    expect(wrapPi(-Math.PI)).toBeCloseTo(Math.PI, 12)
    expect(wrapPi(3 * Math.PI)).toBeCloseTo(Math.PI, 12)
    expect(wrapPi(Math.PI + 0.1)).toBeCloseTo(-Math.PI + 0.1, 12)
    expect(wrapPi(-Math.PI - 0.1)).toBeCloseTo(Math.PI - 0.1, 12)
  })
})
