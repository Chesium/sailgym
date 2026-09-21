import { describe, expect, it } from 'vitest'

import {
  createReplaySource,
  diagnosticsFromFrame,
  replayWindField,
  selectInspection,
  snapshotFromFrame,
  wrapPi,
  LIVE,
} from '../../src/sim/replay'
import { replayChartData, CHART_SERIES } from '../../src/ui/Charts'
import { timelineState } from '../../src/ui/Timeline'
import {
  describeIdentity,
  identityNamesABaseline,
  megabytes,
  recordingSeconds,
  type ExperimentIdentity,
} from '../../src/sim/episodeIo'
import { SNAPSHOT_FIELDS } from '../../src/sim/snapshot'
import type {
  Episode,
  EpisodeFrame,
  EpisodeHeader,
  FrameDiagnostics,
} from '../../src/sim/scenarioTypes'

/**
 * The replay player, the inspection selection, the replay charts and the
 * transport's rule (v1 task 9.4, v2 section 10 tasks 10.3 and 10.4).
 *
 * `sampleAt` is display-only interpolation between recorded frames — it is
 * not physics (F8) and it never feeds the core. What has to be true is that
 * it is *exact* on a frame, *linear* between two, and that the two wrapped
 * angles of F3 take the short way round.
 *
 * What v2 section 10 adds to that is the negative half, and it is the half
 * that matters: a value the episode does not carry must come back **absent**,
 * not zero and not borrowed from the live simulation (RV57, RV59). Every
 * legacy case below is a schema-1 frame — `diag` absent — and the assertions
 * are that the field is missing, not that it is some particular number.
 */

const PSI = SNAPSHOT_FIELDS.indexOf('psi')
const BETA = SNAPSHOT_FIELDS.indexOf('beta')
const X = SNAPSHOT_FIELDS.indexOf('x')
const PHI = SNAPSHOT_FIELDS.indexOf('phi')
const U = SNAPSHOT_FIELDS.indexOf('u')
const V = SNAPSHOT_FIELDS.indexOf('v')
const R = SNAPSHOT_FIELDS.indexOf('r')

/** A schema-2 header, as `EpisodeHeader::manual` writes one. */
const HEADER: EpisodeHeader = {
  schema_version: 2,
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
  identity_version: 1,
  model: { value: { model_version: 2, source: { tree: 'a'.repeat(40), state: 'clean' } } },
  initial_state: { value: {} },
  initial_controls: { value: {} },
  practice: null,
  action: 'not_applicable',
  observation: 'not_applicable',
}

/** The same episode as a schema-1 document: no identity, no diagnostics. */
const LEGACY_HEADER: EpisodeHeader = {
  ...HEADER,
  schema_version: 1,
  identity_version: 0,
  model: 'unknown',
  initial_state: 'unknown',
  initial_controls: 'unknown',
  practice: null,
  action: 'unknown',
  observation: 'unknown',
}

/** A diagnostics block whose every scalar is distinguishable. */
function diag(base: number): FrameDiagnostics {
  const n = (k: number) => base + k
  return {
    wind_speed: n(0),
    wind_bearing_deg: n(1),
    true_wind_body: [n(2), n(3)],
    apparent_wind_body: [n(4), n(5), n(6)],
    apparent_wind_speed: n(7),
    apparent_wind_angle: n(8),
    speed_over_ground: n(9),
    acceleration_body: [n(10), n(11)],
    total_force_h: [n(12), n(13)],
    sheet_force: [n(14), n(15), n(16)],
    sail_ce_b: [n(17), n(18), n(19)],
    board_centre_b: [n(20), n(21), n(22)],
    rudder_centre_b: [n(23), n(24), n(25)],
    sheet_attach_b: [n(26), n(27), n(28)],
    sheet_block_b: [n(29), n(30), n(31)],
    alpha_sail: n(32),
    alpha_board: n(33),
    alpha_rudder: n(34),
    sheet_rope_length: n(35),
    sheet_extension: n(36),
    gz: n(37),
    capsize_since: n(38),
    capsize_max_heel: n(39),
  }
}

/** A frame with `state[i] = base + i`, so every index is distinguishable. */
function frame(
  t: number,
  overrides: Partial<Record<number, number>> = {},
  base = 0,
  withDiag = true,
): EpisodeFrame {
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
    diag: withDiag ? diag(base * 1000) : null,
  }
}

function episode(frames: EpisodeFrame[], header: EpisodeHeader = HEADER): Episode {
  return { header, frames }
}

/** The live `Diagnostics` key list, as `selectInspection` is handed one. */
const LIVE_KEYS = [
  't',
  'steps',
  'true_wind_world',
  'true_wind_body',
  'apparent_wind_body',
  'apparent_wind_speed',
  'apparent_wind_angle',
  'velocity_body',
  'speed_over_ground',
  'course_over_ground',
  'acceleration_body',
  'yaw_rate',
  'roll_rate',
  'leeway_angle',
  'sail',
  'board',
  'rudder',
  'hull',
  'sheet',
  'sheet_hull',
  'total_force_h',
  'yaw_moment',
  'heeling_moment',
  'righting_moment',
  'boom_moment',
  'sail_ce_b',
  'board_centre_b',
  'rudder_centre_b',
  'sheet_attach_b',
  'sheet_block_b',
  'alpha_sail',
  'cl_sail',
  'cd_sail',
  'alpha_board',
  'cl_board',
  'cd_board',
  'alpha_rudder',
  'cl_rudder',
  'cd_rudder',
  'beta',
  'beta_dot',
  'sheet_tension',
  'sheet_rope_length',
  'sheet_extension',
  'gz',
  'heel_deg',
  'capsize',
  'energy_kinetic',
  'energy_roll_potential',
  'energy_sheet_elastic',
  'hull_model_warning',
] as const

/** A live record shaped like one, so `selectInspection` has a key list. */
function liveRecord(): Record<string, unknown> {
  return Object.fromEntries(LIVE_KEYS.map((k) => [k, 1])) as Record<string, unknown>
}

describe('replay source', () => {
  it('returns a frame unchanged at its exact time', () => {
    const frames = [frame(0, {}, 0), frame(0.5, {}, 100), frame(1, {}, 200)]
    const source = createReplaySource(episode(frames))

    expect(source.frameCount).toBe(3)
    expect(source.startTime).toBe(0)
    expect(source.endTime).toBe(1)
    expect(source.logHz).toBe(20)
    expect(source.schemaVersion).toBe(2)

    for (const f of frames) {
      const sample = source.sampleAt(f.t)
      expect(sample).not.toBeNull()
      expect(sample?.t).toBe(f.t)
      expect(sample?.state).toEqual(f.state)
      expect(sample?.forces).toEqual(f.forces)
      expect(sample?.moments).toEqual(f.moments)
      expect(sample?.sheet_tension).toBe(f.sheet_tension)
      expect(sample?.capsized).toBe(f.capsized)
    }

    // Before the start and after the end clamp to the ends rather than
    // extrapolating: a replay has no data outside its own episode.
    expect(source.sampleAt(-10)?.state).toEqual(frames[0].state)
    expect(source.sampleAt(99)?.state).toEqual(frames[2].state)
  })

  it('interpolates linearly between frames, against a hand-computed midpoint', () => {
    // Two frames a second apart. At t = 0.5 every lerped value must be the
    // arithmetic mean, worked out here rather than taken from the source.
    const a = frame(1, { [X]: 10, [PHI]: -0.4 }, 0)
    const b = frame(2, { [X]: 20, [PHI]: 0.6 }, 100)
    const source = createReplaySource(episode([a, b]))

    const mid = source.sampleAt(1.5)
    expect(mid).not.toBeNull()
    if (mid === null) return
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
    expect(quarter?.state[X]).toBeCloseTo(12.5, 12)
    expect(quarter?.state[PHI]).toBeCloseTo(-0.15, 12)

    // Commands and the capsize report are held, not averaged: a rudder
    // command of −1 averaged with +1 would display a helm position that
    // never happened.
    expect(mid.controls).toEqual(a.controls)
    expect(mid.capsized).toBe(a.capsized)

    // **The diagnostics block is not interpolated and not carried.** An
    // averaged force is an evaluation nobody made; the inspection view takes
    // the preceding recorded sample's block instead (task 10.3).
    expect(mid.diag).toBeNull()
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
      if (sample === null || previous === null) {
        throw new Error('the fixture has frames')
      }
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
    expect(Math.abs(source.sampleAt(0.5)?.state[PSI] ?? 0)).toBeCloseTo(Math.PI, 9)

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

  it('makes an empty episode a source with nothing in it, not an exception', () => {
    // v2 section 10 task 10.4: a throw here would have to be caught by every
    // caller and would leave the transport with nothing to render.
    const source = createReplaySource(episode([]))
    expect(source.frameCount).toBe(0)
    expect(source.startTime).toBe(0)
    expect(source.endTime).toBe(0)
    expect(source.frameAt(0)).toBeNull()
    expect(source.indexAt(0)).toBe(-1)
    expect(source.sampleAt(0)).toBeNull()
  })

  it('maps a frame onto the F8.3 snapshot layout', () => {
    const f = frame(3, {}, 0)
    const snapshot = snapshotFromFrame(f)
    SNAPSHOT_FIELDS.forEach((field, i) => {
      expect(snapshot[field]).toBe(f.state[i])
    })
  })
})

describe('recorded frame → diagnostics', () => {
  it('reads the schema-1 fields out of the state and the frame', () => {
    const f = frame(2, {}, 7, false)
    const d = diagnosticsFromFrame(f)

    // Straight out of the recorded state — an index, not a recomputation.
    expect(d.velocity_body).toEqual({ x: f.state[U], y: f.state[V] })
    expect(d.yaw_rate).toBe(f.state[R])
    expect(d.beta).toBe(f.state[SNAPSHOT_FIELDS.indexOf('beta')])
    // `heel_deg` is `φ` in degrees — the one conversion F1 permits at a UI
    // boundary, computed by `units.ts` and not by a second implementation.
    expect(d.heel_deg).toBeCloseTo((f.state[PHI] * 180) / Math.PI, 12)
    // Straight out of the frame.
    expect(d.t).toBe(2)
    expect(d.sheet_tension).toBe(f.sheet_tension)
    expect(d.true_wind_world).toEqual({ x: f.wind_at_boat[0], y: f.wind_at_boat[1] })
    expect(d.yaw_moment).toBe(f.moments[0])
    expect(d.heeling_moment).toBe(f.moments[1])
    expect(d.righting_moment).toBe(f.moments[2])
  })

  it('leaves every schema-2 field absent for a legacy frame', () => {
    const d = diagnosticsFromFrame(frame(2, {}, 7, false))
    // Absent, not zero: `undefined` is what forces a consumer to say
    // "Not recorded" instead of drawing a number nobody measured (RV59).
    for (const key of [
      'apparent_wind_speed',
      'apparent_wind_angle',
      'speed_over_ground',
      'acceleration_body',
      'sail',
      'board',
      'rudder',
      'hull',
      'sheet',
      'total_force_h',
      'sail_ce_b',
      'alpha_sail',
      'sheet_rope_length',
      'gz',
      'capsize',
    ] as const) {
      expect(d[key], `${key} must be absent`).toBeUndefined()
      expect(key in d, `${key} must not even be a key`).toBe(false)
    }
  })

  it('unpacks the whole block for a schema-2 frame, in the declared order', () => {
    const f = frame(2, {}, 1)
    const d = diagnosticsFromFrame(f)
    const b = f.diag as FrameDiagnostics

    expect(d.apparent_wind_speed).toBe(b.apparent_wind_speed)
    expect(d.apparent_wind_body).toEqual({
      x: b.apparent_wind_body[0],
      y: b.apparent_wind_body[1],
      z: b.apparent_wind_body[2],
    })
    expect(d.acceleration_body).toEqual({
      x: b.acceleration_body[0],
      y: b.acceleration_body[1],
    })
    // The four component loads are a recorded force and a recorded arm.
    expect(d.sail?.f).toEqual({ x: f.forces[0], y: f.forces[1], z: f.forces[2] })
    expect(d.sail?.r).toEqual({ x: b.sail_ce_b[0], y: b.sail_ce_b[1], z: b.sail_ce_b[2] })
    expect(d.board?.f).toEqual({ x: f.forces[3], y: f.forces[4], z: f.forces[5] })
    expect(d.rudder?.f).toEqual({ x: f.forces[6], y: f.forces[7], z: f.forces[8] })
    expect(d.hull?.f).toEqual({ x: f.forces[9], y: f.forces[10], z: f.forces[11] })
    // F6.6: the hull force acts at the CG, so its arm is structurally zero.
    expect(d.hull?.r).toEqual({ x: 0, y: 0, z: 0 })
    // The sheet's arm is its boom attachment, which the block records.
    expect(d.sheet?.r).toEqual({
      x: b.sheet_attach_b[0],
      y: b.sheet_attach_b[1],
      z: b.sheet_attach_b[2],
    })
    expect(d.gz).toBe(b.gz)
    expect(d.capsize).toEqual({
      capsized: f.capsized,
      since: b.capsize_since,
      max_heel: b.capsize_max_heel,
    })
  })

  it('never fills the omitted diagnostics, even with the block present', () => {
    // `docs/v2/recording-format.md` §4's omission list, on the browser side.
    const d = diagnosticsFromFrame(frame(2, {}, 1))
    for (const key of [
      'steps',
      'course_over_ground',
      'leeway_angle',
      'cl_sail',
      'cd_sail',
      'cl_board',
      'cd_board',
      'cl_rudder',
      'cd_rudder',
      'boom_moment',
      'sheet_hull',
      'energy_kinetic',
      'energy_roll_potential',
      'energy_sheet_elastic',
      'hull_model_warning',
    ] as const) {
      expect(key in d, `${key} is omitted by the schema and must stay absent`).toBe(false)
    }
  })
})

describe('the one display selection', () => {
  it('takes every number from the episode, and none from the live record', () => {
    const frames = [frame(0, {}, 1), frame(1, {}, 2)]
    const source = createReplaySource(episode(frames))
    const live = liveRecord() as never

    const view = selectInspection({ kind: 'replay', source }, 0.5, {
      snapshot: snapshotFromFrame(frame(99, {}, 500)),
      diagnostics: live,
      wind: { source: 'live', wx: 9, wy: 9, speed: 9, bearingDeg: 9 },
    })

    expect(view.source).toBe('replay')
    // The pose is the interpolated playhead…
    expect(view.t).toBeCloseTo(0.5, 12)
    expect(view.interpolated).toBe(true)
    // …and the diagnostics are the **preceding recorded sample**, with that
    // sample's own time, not the playhead's.
    expect(view.sampleIndex).toBe(0)
    expect(view.diagnostics?.source).toBe('recorded')
    expect(view.diagnostics?.t).toBe(0)
    expect(view.diagnostics?.values.sheet_tension).toBe(frames[0].sheet_tension)
    // The live record's marker value (1 everywhere) reaches nothing.
    expect(view.diagnostics?.values.gz).not.toBe(1)
    expect(view.wind?.source).toBe('recorded')
    expect(view.wind?.wx).toBe(frames[0].wind_at_boat[0])
    expect(view.wind?.speed).toBe((frames[0].diag as FrameDiagnostics).wind_speed)
    // Controls are held from that sample, never averaged.
    expect(view.controls?.rudderRateCmd).toBe(frames[0].controls[0])
  })

  it('reports exactly the fields the episode does not carry', () => {
    const live = liveRecord() as never
    const modern = createReplaySource(episode([frame(0, {}, 1), frame(1, {}, 2)]))
    const legacy = createReplaySource(
      episode([frame(0, {}, 1, false), frame(1, {}, 2, false)], LEGACY_HEADER),
    )
    const call = (source: ReturnType<typeof createReplaySource>) =>
      selectInspection({ kind: 'replay', source }, 0, {
        snapshot: snapshotFromFrame(frame(0, {}, 0)),
        diagnostics: live,
        wind: null,
      })

    // Schema 2: only the fifteen the format deliberately omits.
    expect([...call(modern).unavailable]).toEqual([
      'boom_moment',
      'cd_board',
      'cd_rudder',
      'cd_sail',
      'cl_board',
      'cl_rudder',
      'cl_sail',
      'course_over_ground',
      'energy_kinetic',
      'energy_roll_potential',
      'energy_sheet_elastic',
      'hull_model_warning',
      'leeway_angle',
      'sheet_hull',
      'steps',
    ])

    // Schema 1: those fifteen plus everything the block would have carried.
    const old = call(legacy).unavailable
    expect(old.length).toBeGreaterThan(30)
    for (const key of ['alpha_sail', 'gz', 'capsize', 'sail', 'speed_over_ground']) {
      expect(old).toContain(key)
    }
    // …and never the fields a schema-1 frame does carry.
    for (const key of ['t', 'velocity_body', 'yaw_rate', 'sheet_tension', 'heel_deg']) {
      expect(old).not.toContain(key)
    }
  })

  it('renders nothing from a recording when the episode is empty', () => {
    const source = createReplaySource(episode([]))
    const view = selectInspection({ kind: 'replay', source }, 0, {
      snapshot: snapshotFromFrame(frame(0, {}, 0)),
      diagnostics: liveRecord() as never,
      wind: null,
    })
    expect(view.emptyEpisode).toBe(true)
    expect(view.diagnostics).toBeNull()
    expect(view.wind).toBeNull()
    expect(view.controls).toBeNull()
    expect(view.unavailable.length).toBe(LIVE_KEYS.length)
  })

  it('passes the live record straight through when live', () => {
    const live = liveRecord() as never
    const snapshot = snapshotFromFrame(frame(4, {}, 3))
    const view = selectInspection(LIVE, 0, {
      snapshot,
      diagnostics: live,
      wind: { source: 'live', wx: 1, wy: 2, speed: 3, bearingDeg: 4 },
    })
    expect(view.source).toBe('live')
    expect(view.interpolated).toBe(false)
    expect(view.sampleIndex).toBeNull()
    expect(view.diagnostics?.source).toBe('live')
    expect(view.diagnostics?.values).toBe(live)
    expect(view.unavailable).toEqual([])
    expect(view.windField.available).toBe(true)
  })

  it('is exactly on a sample when the playhead is', () => {
    const frames = [frame(0, {}, 1), frame(1, {}, 2)]
    const source = createReplaySource(episode(frames))
    const at = (t: number) =>
      selectInspection({ kind: 'replay', source }, t, {
        snapshot: snapshotFromFrame(frame(0, {}, 0)),
        diagnostics: liveRecord() as never,
        wind: null,
      })
    expect(at(1).interpolated).toBe(false)
    expect(at(1).sampleIndex).toBe(1)
    expect(at(0).interpolated).toBe(false)
    expect(at(0.5).interpolated).toBe(true)
  })
})

describe('the spatial wind field in replay', () => {
  it('is hidden, and says which condition failed', () => {
    // A schema-1 episode: it records no identity at all.
    const legacy = replayWindField(LEGACY_HEADER)
    expect(legacy.available).toBe(false)
    expect(legacy.reason).toContain('schema 1')

    // A schema-2 episode from a dirty tree: it names no baseline (F18.1d).
    const dirty = replayWindField({
      ...HEADER,
      model: { value: { model_version: 2, source: { tree: 'b'.repeat(40), state: 'dirty' } } },
    })
    expect(dirty.available).toBe(false)
    expect(dirty.reason).toContain('baseline')

    // A clean schema-2 episode: the reason is about **this build**, which does
    // not reconstruct a recorded field. The recorded vector at the boat does
    // not identify the field (RV57).
    const clean = replayWindField(HEADER)
    expect(clean.available).toBe(false)
    expect(clean.reason).toContain('does not reconstruct')
  })
})

describe('replay charts', () => {
  const frames = [frame(0, {}, 1), frame(0.5, {}, 2), frame(1, {}, 3), frame(1.5, {}, 4)]
  const source = createReplaySource(episode(frames))

  it('plots recorded samples bounded by the playhead, never an interpolation', () => {
    const data = replayChartData(source, 1)
    // Three samples: t = 0, 0.5 and 1. The bound is inclusive.
    expect(data.samples.map((s) => s.t)).toEqual([0, 0.5, 1])
    expect(data.span).toBe(1)
    // Every timestamp is a **recorded** one. A playhead between samples adds
    // no point, which is what "no score from an interpolated frame" means.
    const between = replayChartData(source, 0.75)
    expect(between.samples.map((s) => s.t)).toEqual([0, 0.5])
    for (const s of between.samples) {
      expect(frames.some((f) => f.t === s.t)).toBe(true)
    }
    // Ordered, strictly increasing.
    for (let i = 1; i < data.samples.length; i += 1) {
      expect(data.samples[i].t).toBeGreaterThan(data.samples[i - 1].t)
    }
  })

  it('is a pure function: playing twice or scrubbing back gives identical points', () => {
    const first = replayChartData(source, 1)
    // Scrub away, scrub back, render a few more times.
    replayChartData(source, 0)
    replayChartData(source, 1.5)
    replayChartData(source, 0.25)
    const again = replayChartData(source, 1)
    expect(again.samples).toEqual(first.samples)
    expect(again.span).toBe(first.span)
    expect(again.samples.length).toBe(3)
    // …and going backwards shortens the series rather than duplicating it.
    expect(replayChartData(source, 0.5).samples.length).toBe(2)
  })

  it('holds the discrete values and reads the wrapped ones from the state', () => {
    const data = replayChartData(source, 1.5)
    const heel = data.samples.map((s) => s.values.heelDeg)
    // Heel is `φ` in degrees, and `φ` is not wrapped (F3): the fixture's
    // `state[3]` climbs by 1000 per frame, so the series climbs with it and is
    // never folded back into ±180.
    expect(heel.every((v) => v !== undefined)).toBe(true)
    expect(heel[3]).toBeGreaterThan(heel[0] as number)
    // The sheet tension series is the frame's own scalar, held per sample.
    expect(data.samples.map((s) => s.values.sheetTension)).toEqual([3, 6, 9, 12])
  })

  it('leaves a series absent when the episode does not record it', () => {
    const legacy = createReplaySource(
      episode(
        [frame(0, {}, 1, false), frame(0.5, {}, 2, false)],
        LEGACY_HEADER,
      ),
    )
    const data = replayChartData(legacy, 1)
    expect(data.samples.length).toBe(2)
    for (const s of data.samples) {
      // Recorded in schema 1…
      expect(s.values.heelDeg).toBeDefined()
      expect(s.values.sheetTension).toBeDefined()
      expect(s.values.yawRate).toBeDefined()
      expect(s.values.rollRate).toBeDefined()
      expect(s.values.rightingMoment).toBeDefined()
      // …and not.
      expect(s.values.boatSpeed).toBeUndefined()
      expect(s.values.alphaSail).toBeUndefined()
      expect(s.values.apparentWindSpeed).toBeUndefined()
    }
  })

  it('reads every series through a PartialDiagnostics without throwing', () => {
    // Guards the series table itself: a `read` written as `d.x.y` would throw
    // on an absent field rather than returning `undefined`.
    for (const series of CHART_SERIES) {
      expect(() => series.read({})).not.toThrow()
      expect(series.read({})).toBeUndefined()
    }
  })

  it('has no points at all for an empty episode', () => {
    const data = replayChartData(createReplaySource(episode([])), 5)
    expect(data.samples).toEqual([])
    expect(data.span).toBe(0)
  })
})

describe('the transport', () => {
  it('disables everything for an empty episode', () => {
    const state = timelineState(createReplaySource(episode([])), 3)
    expect(state.frames).toBe(0)
    expect(state.index).toBe(-1)
    expect(state.span).toBe(0)
    expect(state.scrubDisabled).toBe(true)
    expect(state.playDisabled).toBe(true)
    expect(state.canStepBack).toBe(false)
    expect(state.canStepForward).toBe(false)
    expect(state.label).toBe('no samples')
  })

  it('makes a one-frame episode a constant timeline', () => {
    const source = createReplaySource(episode([frame(2.5, {}, 1)]))
    const state = timelineState(source, 99)
    expect(state.frames).toBe(1)
    expect(state.span).toBe(0)
    expect(state.time).toBe(2.5)
    expect(state.index).toBe(0)
    expect(state.scrubDisabled).toBe(true)
    expect(state.playDisabled).toBe(true)
    expect(state.label).toContain('one sample')
    // Clamped from both sides, to the one sample's own time.
    expect(timelineState(source, -99).time).toBe(2.5)
  })

  it('clamps predictably before the start and after the end', () => {
    const source = createReplaySource(episode([frame(1, {}, 1), frame(2, {}, 2), frame(3, {}, 3)]))
    expect(timelineState(source, -5).time).toBe(1)
    expect(timelineState(source, -5).index).toBe(0)
    expect(timelineState(source, 500).time).toBe(3)
    expect(timelineState(source, 500).index).toBe(2)
    expect(timelineState(source, Number.NaN).time).toBe(1)
    // The steps are bounded by the episode, in both directions.
    expect(timelineState(source, 1).canStepBack).toBe(false)
    expect(timelineState(source, 1).canStepForward).toBe(true)
    expect(timelineState(source, 3).canStepBack).toBe(true)
    expect(timelineState(source, 3).canStepForward).toBe(false)
    expect(timelineState(source, 2).span).toBe(2)
    expect(timelineState(source, 2).label).toContain('frame 2/3')
  })
})

describe('episode identity, on the browser side', () => {
  const clean: ExperimentIdentity = {
    identity_version: 1,
    model: { value: { model_version: 2, source: { tree: 'a'.repeat(40), state: 'clean' } } },
    parameters: { value: {} },
    integrator: { value: 'Rk2Midpoint' },
    dt: { value: 0.005 },
    initial_state: { value: {} },
    initial_controls: { value: {} },
    scenario: { value: 'free_sail' },
    wind: { value: {} },
    seed: { value: 0 },
    task: 'not_applicable',
    action: 'not_applicable',
    observation: 'not_applicable',
  }

  it('calls an episode a baseline only when it names one', () => {
    expect(identityNamesABaseline(clean)).toBe(true)
    expect(describeIdentity(clean)).toContain('model v2')

    // A schema-1 episode: viewable, and never a same-conditions experiment.
    const legacy: ExperimentIdentity = { ...clean, identity_version: 0, model: 'unknown' }
    expect(identityNamesABaseline(legacy)).toBe(false)
    expect(describeIdentity(legacy)).toContain('schema-1')

    // A dirty tree names no baseline either — F18.1d, and it is why a dirty
    // identity is not comparable even with itself.
    const dirty: ExperimentIdentity = {
      ...clean,
      model: { value: { model_version: 2, source: { tree: 'a'.repeat(40), state: 'dirty' } } },
    }
    expect(identityNamesABaseline(dirty)).toBe(false)
    expect(describeIdentity(dirty)).toContain('names no baseline')
  })
})

describe('the recording bound', () => {
  it('reports the duration and the size the cap implies', () => {
    // The numbers are the core's; these are the conversions the readout does.
    const limit = { frames: 13_443, bytesPerFrame: 624, bytes: 13_443 * 624 }
    expect(recordingSeconds(limit, 20)).toBeCloseTo(672.15, 2)
    expect(recordingSeconds(limit, 50)).toBeCloseTo(268.86, 2)
    expect(recordingSeconds(limit, 0)).toBe(0)
    expect(megabytes(limit.bytes)).toBe('8.0 MB')
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
