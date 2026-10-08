import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'

import {
  DEFAULT_SCENARIO,
  EPISODE_SCHEMA_VERSION,
  IDENTITY_VERSION,
  PRACTICE_ENVELOPE_VERSION,
  SCENARIO_SCHEMA_VERSION,
  SUPPORTED_SCHEMA_VERSIONS,
  summarise,
  type BaselineRun,
  type CourseBlock,
  type EpisodeComparison,
  type PracticeChallenge,
  type PracticeState,
  type Scenario,
} from '../../src/sim/scenarioTypes'

/**
 * Rust ⇄ TypeScript parity for the scenario schema and the recording schema
 * (tasks 9.1, 9.3), asserted the same way `diagnostics.test.ts` does it for
 * the debug record.
 *
 * Both sides are parsed out of their own declarations and compared as
 * **sets, in both directions**. A field added in Rust and forgotten here is a
 * failing test rather than a silently missing control — which matters more
 * here than anywhere else, because brief §45 makes this schema the forward
 * interface to the episode inspector and to RL.
 *
 * The six shipped documents are parsed too: this is the browser-side half of
 * `scenario::shipped::no_scripted_outcomes`.
 */

const SCENARIO_RS = '../crates/sailgym-physics/src/scenario.rs'
const RECORDING_RS = '../crates/sailgym-physics/src/recording.rs'
const TS = 'src/sim/scenarioTypes.ts'
const SCENARIO_DIR = '../scenarios'

/** The body of a braced block that starts at `opening`, to its closing brace. */
function block(source: string, opening: string): string {
  const start = source.indexOf(opening)
  expect(start, `${opening} must be declared`).toBeGreaterThanOrEqual(0)
  const from = start + opening.length
  const end = source.indexOf('\n}', from)
  expect(end, `${opening} must be closed`).toBeGreaterThan(from)
  return source.slice(from, end)
}

/** `pub name: Type,` declarations, one per field. */
function rustFields(file: string, opening: string): string[] {
  return [...block(readFileSync(file, 'utf8'), opening).matchAll(/^\s*pub (\w+):/gm)].map(
    (m) => m[1],
  )
}

/** `  name: Type` declarations at the interface's own indentation level. */
function tsFields(opening: string): string[] {
  return [...block(readFileSync(TS, 'utf8'), opening).matchAll(/^ {2}(\w+)[?]?:/gm)].map(
    (m) => m[1],
  )
}

const PAIRS: ReadonlyArray<[string, string, string]> = [
  ['Scenario', SCENARIO_RS, 'pub struct Scenario {'],
  ['InitialState', SCENARIO_RS, 'pub struct InitialState {'],
  ['EpisodeHeader', RECORDING_RS, 'pub struct EpisodeHeader {'],
  ['EpisodeFrame', RECORDING_RS, 'pub struct EpisodeFrame {'],
  ['FrameDiagnostics', RECORDING_RS, 'pub struct FrameDiagnostics {'],
  ['ToolchainInfo', RECORDING_RS, 'pub struct ToolchainInfo {'],
  ['Episode', RECORDING_RS, 'pub struct Episode {'],
]

describe('scenario and episode field parity', () => {
  it('parses a non-trivial field list from both sides', () => {
    // Guards the parsers themselves: a regex that silently matched nothing
    // would make every comparison below vacuously true.
    for (const [name, file, opening] of PAIRS) {
      expect(rustFields(file, opening).length, `${name} (Rust)`).toBeGreaterThan(1)
      expect(tsFields(`export interface ${name} {`).length, `${name} (TS)`).toBeGreaterThan(1)
    }
    expect(rustFields(SCENARIO_RS, 'pub struct Scenario {')).toHaveLength(9)
    // Schema 2 added `diag` to the frame and seven header fields; both
    // counts are pinned so a silent addition on either side fails here.
    expect(rustFields(RECORDING_RS, 'pub struct EpisodeFrame {')).toHaveLength(10)
    expect(rustFields(RECORDING_RS, 'pub struct EpisodeHeader {')).toHaveLength(14)
    expect(rustFields(RECORDING_RS, 'pub struct FrameDiagnostics {')).toHaveLength(23)
  })

  it('has the same fields on both sides, in the same order', () => {
    for (const [name, file, opening] of PAIRS) {
      const rust = rustFields(file, opening)
      const ts = tsFields(`export interface ${name} {`)
      expect(ts.filter((f) => !rust.includes(f)), `${name}: extra in TypeScript`).toEqual([])
      expect(rust.filter((f) => !ts.includes(f)), `${name}: missing from TypeScript`).toEqual([])
      expect(ts, `${name}: order`).toEqual(rust)
    }
  })

  /**
   * The identity records are written on one line each in TypeScript, by the
   * same convention the scenario helpers use, so {@link block} cannot bracket
   * them. They are compared by name instead — which is what the field lists
   * are for.
   */
  it('mirrors the identity records, which are one-line declarations', () => {
    const ts = readFileSync(TS, 'utf8')
    const ONE_LINERS: ReadonlyArray<[string, string]> = [
      ['TaskIdentity', 'pub struct TaskIdentity {'],
      ['ActionIdentity', 'pub struct ActionIdentity {'],
      ['ObservationField', 'pub struct ObservationField {'],
      ['ObservationIdentity', 'pub struct ObservationIdentity {'],
      ['PracticeEvent', 'pub struct PracticeEvent {'],
      ['PracticeEnvelope', 'pub struct PracticeEnvelope {'],
    ]
    for (const [name, opening] of ONE_LINERS) {
      const rust = rustFields(RECORDING_RS, opening)
      expect(rust.length, `${name} (Rust)`).toBeGreaterThan(1)
      const line = ts
        .split('\n')
        .find((l) => l.startsWith(`export interface ${name} {`))
      expect(line, `${name} must be declared on one line in ${TS}`).toBeDefined()
      const declared = [...(line ?? '').matchAll(/(\w+)[?]?:/g)].map((m) => m[1])
      expect(declared, `${name}: fields and order`).toEqual(rust)
    }
  })

  it('mirrors the schema versions and the default scenario id', () => {
    const recording = readFileSync(RECORDING_RS, 'utf8')
    expect(recording).toContain(`pub const IDENTITY_VERSION: u32 = ${IDENTITY_VERSION};`)
    expect(recording).toContain(
      `pub const PRACTICE_ENVELOPE_VERSION: u32 = ${PRACTICE_ENVELOPE_VERSION};`,
    )
    // The read set, spelled the same on both sides. A schema this build can
    // no longer read is a recording someone has lost (RV60).
    expect(recording).toContain(
      `pub const SUPPORTED_SCHEMA_VERSIONS: [u32; ${SUPPORTED_SCHEMA_VERSIONS.length}] = [${SUPPORTED_SCHEMA_VERSIONS.join(', ')}];`,
    )
    expect(SUPPORTED_SCHEMA_VERSIONS).toContain(EPISODE_SCHEMA_VERSION)
    expect(SUPPORTED_SCHEMA_VERSIONS).toContain(1)
    const scenarioRs = readFileSync(SCENARIO_RS, 'utf8')
    const recordingRs = readFileSync(RECORDING_RS, 'utf8')
    expect(scenarioRs).toContain(
      `pub const SCENARIO_SCHEMA_VERSION: u32 = ${SCENARIO_SCHEMA_VERSION};`,
    )
    expect(recordingRs).toContain(
      `pub const EPISODE_SCHEMA_VERSION: u32 = ${EPISODE_SCHEMA_VERSION};`,
    )
    expect(scenarioRs).toContain(`pub const DEFAULT_SCENARIO: &str = '${DEFAULT_SCENARIO}'`
      .replace(/'/g, '"'))
  })
})

describe('the six shipped scenarios', () => {
  const NAMES = [
    'beam_reach_capsize',
    'close_hauled',
    'free_sail',
    'gybe',
    'sheet_release_recovery',
    'tack',
  ] as const

  const load = (name: string): Scenario =>
    JSON.parse(readFileSync(`${SCENARIO_DIR}/${name}.json`, 'utf8')) as Scenario

  it('are exactly the six of brief §32, and each is its own file stem', () => {
    for (const name of NAMES) {
      const scenario = load(name)
      expect(scenario.name).toBe(name)
      expect(scenario.schema_version).toBe(SCENARIO_SCHEMA_VERSION)
      expect(scenario.description.length).toBeGreaterThan(20)
      expect(scenario.initial_state.sheet_length).toBeGreaterThanOrEqual(0)
      expect(scenario.wind.speed).toBeGreaterThanOrEqual(0)
      expect(scenario.camera.zoom).toBeGreaterThan(0)
      expect(['follow', 'northUp']).toContain(scenario.camera.mode)
    }
  })

  it('script no outcomes', () => {
    // brief §32, last line. The Rust half of this is
    // `scenario::shipped::no_scripted_outcomes`; this is the same check over
    // the files as they sit on disk.
    for (const name of NAMES) {
      const text = readFileSync(`${SCENARIO_DIR}/${name}.json`, 'utf8')
      const keys = [...text.matchAll(/"([^"]+)"\s*:/g)].map((m) => m[1].toLowerCase())
      expect(keys.length, `${name}: the key scan found nothing`).toBeGreaterThan(5)
      for (const key of keys) {
        // `description` is the one schema key that contains one of the five
        // words as a substring (de-`script`-ion), and it is prose.
        if (key === 'description') {
          continue
        }
        for (const forbidden of ['script', 'sequence', 'events', 'timeline', 'forced']) {
          expect(key, `${name} carries a ${key} key`).not.toContain(forbidden)
        }
      }
      expect(load(name).initial_controls).toBeNull()
    }
  })

  it('makes sheet_release_recovery physically identical to beam_reach_capsize', () => {
    // brief §46 demonstrates capsize and recovery from the *same* setup, so
    // if the two files differ in any physical field the demonstration proves
    // nothing.
    const capsize = load('beam_reach_capsize')
    const recovery = load('sheet_release_recovery')
    expect(recovery.seed).toBe(capsize.seed)
    expect(recovery.parameter_overrides).toEqual(capsize.parameter_overrides)
    expect(recovery.initial_state).toEqual(capsize.initial_state)
    expect(recovery.wind).toEqual(capsize.wind)
    expect(recovery.initial_controls).toEqual(capsize.initial_controls)
    expect(recovery.camera).toEqual(capsize.camera)
    // …and the only things that differ are the two labels.
    expect(recovery.name).not.toBe(capsize.name)
    expect(recovery.description).not.toBe(capsize.description)
  })

  it('has free_sail as the default, at the wind speed the app expects', () => {
    expect(DEFAULT_SCENARIO).toBe('free_sail')
    const free = load(DEFAULT_SCENARIO)
    // `sail.spec.ts` builds its own comparison `Sim` with a uniform 5 m/s
    // northerly, so the default scenario has to be exactly that.
    expect(free.wind.mode).toBe('uniform')
    expect(free.wind.speed).toBe(5)
    expect(free.wind.bearing_deg).toBe(0)
    expect(free.initial_state.speed).toBe(0)
    expect(free.camera.mode).toBe('northUp')
    expect(free.camera.zoom).toBe(1)
  })

  it('summarises into picker rows in the order given', () => {
    const rows = summarise(NAMES.map(load))
    expect(rows.map((r) => r.id)).toEqual([...NAMES])
    expect(rows.every((r) => r.description.length > 0)).toBe(true)
  })
})

// ---------------------------------------------------------------------------
// The section 12 shapes (task 12.7, D5)
// ---------------------------------------------------------------------------

/**
 * The extended JSON shapes, parsed.
 *
 * These are **not** recording-document types, so the field-parity sweep above
 * does not cover them: they are assembled in `crates/sailgym-wasm/src/lib.rs`
 * from the task, course and env crates' public accessors. What this asserts is
 * that the TypeScript declarations parse the shapes the Rust actually emits —
 * the fixtures below are copied from the `serde_json::json!` literals in that
 * file, so a field renamed on one side and not the other fails here.
 *
 * The **values** in them are Rust's and are asserted in the browser, by
 * `tests/e2e/course.spec.ts` against a real `Sim` (task 12.9). This file is
 * about shape.
 */
describe('the section 12 JSON shapes', () => {
  const COURSE_BLOCK: CourseBlock = {
    waypoints: [
      {
        n: 1,
        x: 30,
        y: 0,
        radius: 5,
        posts: [
          [30, -5],
          [30, 5],
        ],
        state: 'next',
      },
      {
        n: 2,
        x: 60,
        y: -10,
        radius: 5,
        posts: [
          [58.41886116991581, -14.743416490252569],
          [61.58113883008419, -5.256583509747431],
        ],
        state: 'pending',
      },
    ],
    start: { x: 0, y: 0 },
    next: 1,
    distance_to_next: 29.5,
    splits: [],
  }

  it('parses a course challenge row, skill and course alike', () => {
    const rows = JSON.parse(
      JSON.stringify([
        {
          id: 'get_moving',
          kind: 'skill',
          version: 1,
          scenario: 'free_sail',
          time_limit_s: 45,
          highlight_event: 'speed_reached',
          metric: { id: 'top_speed', unit: 'm/s' },
          thresholds: { target_speed_mps: 1.2 },
        },
        {
          id: 'course_reach',
          kind: 'course',
          version: 1,
          scenario: 'free_sail',
          time_limit_s: 130,
          highlight_event: 'waypoint_missed',
          metric: { id: 'course_time', unit: 's' },
          thresholds: { 'course.half_width': 5, 'course.waypoint_count': 3 },
          title: 'Reach',
          description: 'Steering and trim only.',
          waypoints: 3,
        },
      ]),
    ) as PracticeChallenge[]
    expect(rows).toHaveLength(2)
    expect(rows[0].kind).toBe('skill')
    expect(rows[0].waypoints).toBeUndefined()
    expect(rows[1].kind).toBe('course')
    expect(rows[1].waypoints).toBe(3)
    // The geometry travels in the thresholds, which is what makes the recorded
    // identity self-describing (D5, F18.3).
    expect(rows[1].thresholds['course.half_width']).toBe(5)
    // The courses are listed after the skills.
    expect(rows.map((r) => r.kind)).toEqual(['skill', 'course'])
  })

  it('parses the practice state with a course block', () => {
    const state = JSON.parse(
      JSON.stringify({
        active: true,
        status: 'active',
        report: {
          task: { id: 'course_reach', version: 1, thresholds: {} },
          outcome: { kind: 'running' },
          scenario: 'free_sail',
          elapsed_s: 1.5,
          elapsed_steps: 300,
          metric: { id: 'course_time', unit: 's', value: 0 },
          progress: { phase: 'sailing', value: 0, target: 3, hold_s: 0, hold_target_s: 0 },
          events: [],
          highlight: null,
        },
        course: COURSE_BLOCK,
      }),
    ) as PracticeState
    expect(state.active).toBe(true)
    if (!state.active) {
      throw new Error('unreachable')
    }
    expect(state.course?.waypoints).toHaveLength(2)
    expect(state.course?.waypoints[0].state).toBe('next')
    expect(state.course?.waypoints[0].posts[0]).toEqual([30, -5])
    expect(state.course?.start).toEqual({ x: 0, y: 0 })
    expect(state.course?.next).toBe(1)
    // A skill attempt carries no block, and that is a `null` rather than a
    // missing key.
    const skill = JSON.parse(
      JSON.stringify({ active: true, status: 'active', report: state.report, course: null }),
    ) as PracticeState
    if (!skill.active) {
      throw new Error('unreachable')
    }
    expect(skill.course).toBeNull()
  })

  it('parses a baseline run', () => {
    const run = JSON.parse(
      JSON.stringify({
        episode: { header: {}, frames: [] },
        narration: [
          { t: 0.05, mode: 'fetching', side: -1, waypoint: 1 },
          { t: 15.05, mode: 'tacking', side: -1, waypoint: 1 },
        ],
        conditions: { verdict: 'same_conditions', reasons: [], describe: 'same conditions' },
        outcome: 'finished',
        task_outcome: 'succeeded',
        time_s: 42.5,
        splits: [14.85, 27.35, 42.5],
      }),
    ) as BaselineRun
    expect(run.narration.map((n) => n.mode)).toEqual(['fetching', 'tacking'])
    expect(run.conditions.verdict).toBe('same_conditions')
    expect(run.time_s).toBe(42.5)
    expect(run.splits).toHaveLength(3)
    // The one word that licenses showing the ghost and the splits (RV70).
    expect(run.conditions.verdict === 'same_conditions').toBe(true)
  })

  it('parses a comparability answer with both verdicts', () => {
    const answer = JSON.parse(
      JSON.stringify({
        verdict: 'different',
        reasons: ['action', 'observation'],
        describe: 'different conditions: action, observation',
        conditions: { verdict: 'same_conditions', reasons: [], describe: 'same conditions' },
      }),
    ) as EpisodeComparison
    // The motivating case, exactly (D4): `compare` refuses a hand-flown
    // attempt against an agent run, and `compare_conditions` does not.
    expect(answer.verdict).toBe('different')
    expect(answer.reasons).toEqual(['action', 'observation'])
    expect(answer.conditions.verdict).toBe('same_conditions')
  })

  it('parses a recorded course, and a `null` for an episode that has none', () => {
    const recorded = JSON.parse(JSON.stringify(COURSE_BLOCK)) as CourseBlock | null
    expect(recorded?.waypoints).toHaveLength(2)
    const none = JSON.parse('null') as CourseBlock | null
    expect(none).toBeNull()
  })
})
