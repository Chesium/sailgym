import { memo, useRef } from 'react'

import { RingBuffer } from './ringBuffer'
import { CHART_CAPACITY, CHART_KEYS, SAMPLE_HZ_CHOICES, type ChartKey } from './store'
import { NOT_RECORDED, type Diagnostics, type PartialDiagnostics } from '../sim/diagnostics'
import { diagnosticsFromFrame, type ReplaySource } from '../sim/replay'

/**
 * Scrolling time-series for a selected subset of the diagnostics (task 8.4).
 *
 * ## Sampling
 *
 * Samples are taken on **simulated** time, at `sampleHz`, from the
 * diagnostics record the frame loop already published — not once per physics
 * step (that would be 200 Hz of samples nobody can see) and not once per
 * frame (that would make the history depend on the machine's frame rate).
 *
 * Nothing here calls across the WASM boundary, starts a timer or touches the
 * clock, so sampling cannot perturb physics timing: the worst it can do is
 * cost a few microseconds of the frame it is already inside. At 4× speed the
 * simulated clock outruns the frame rate, so the gate is "at most one sample
 * per frame, and only once `1/hz` of simulated time has passed" — the series
 * thins out rather than the physics slowing down.
 *
 * ## Why one buffer per series
 *
 * A fixed-capacity {@link RingBuffer} per series, allocated once and reused,
 * so a page left running overnight uses the same memory as one left running
 * for a minute.
 *
 * ## Replay plots the episode, not the page's history
 *
 * v2 section 10 task 10.4: while a recorded episode is being inspected the
 * plots are rebuilt by {@link replayChartData} from **that episode's own
 * samples**, bounded by the playhead, and the live sampler is frozen —
 * {@link useChartSampler}'s `live` flag stops it appending and stops it
 * clearing. Three consequences, all of them acceptance criteria:
 *
 * * repeated renders and backward scrubs cannot append wall-clock samples or
 *   duplicate history, because the series is a pure function of the source and
 *   the playhead and nothing accumulates;
 * * the live history is neither carried into playback nor destroyed by it, so
 *   leaving replay finds the chart exactly as it was;
 * * **no point is interpolated.** Chart points are recorded samples at their
 *   own timestamps. A score computed from an interpolated frame would be a
 *   score for a state nobody simulated, which is what this rule prevents.
 *
 * A series a legacy episode does not carry is drawn empty and labelled
 * `Not recorded` rather than plotted as zero.
 */

export interface ChartSeriesDef {
  key: ChartKey
  label: string
  unit: string
  colour: string
  /**
   * Pull the plotted number out of the record, or `undefined` when the record
   * does not carry it.
   *
   * The record is a {@link PartialDiagnostics}, so a series backed by a field
   * a schema-1 episode does not record simply has no points — it is never
   * plotted as zero and never filled from the live simulation (RV59).
   */
  read: (d: PartialDiagnostics) => number | undefined
}

export const CHART_SERIES: readonly ChartSeriesDef[] = [
  { key: 'heelDeg', label: 'Heel', unit: '°', colour: '#b03030', read: (d) => d.heel_deg },
  {
    key: 'sheetTension',
    label: 'Sheet tension',
    unit: 'N',
    colour: '#0f8b8d',
    read: (d) => d.sheet_tension,
  },
  {
    key: 'boatSpeed',
    label: 'Boat speed',
    unit: 'm/s',
    colour: '#2b8a3e',
    read: (d) => d.speed_over_ground,
  },
  {
    key: 'alphaSail',
    label: 'Sail angle of attack',
    unit: 'rad',
    colour: '#d64d3f',
    read: (d) => d.alpha_sail,
  },
  { key: 'yawRate', label: 'Yaw rate', unit: 'rad/s', colour: '#c2571a', read: (d) => d.yaw_rate },
  { key: 'rollRate', label: 'Roll rate', unit: 'rad/s', colour: '#8c6bb1', read: (d) => d.roll_rate },
  {
    key: 'apparentWindSpeed',
    label: 'Apparent wind',
    unit: 'm/s',
    colour: '#5ab4e0',
    read: (d) => d.apparent_wind_speed,
  },
  {
    key: 'rightingMoment',
    label: 'Righting moment',
    unit: 'N·m',
    colour: '#2b6fb0',
    read: (d) => d.righting_moment,
  },
]

const SERIES_BY_KEY: Record<ChartKey, ChartSeriesDef> = Object.fromEntries(
  CHART_SERIES.map((s) => [s.key, s]),
) as Record<ChartKey, ChartSeriesDef>

export interface Sample {
  t: number
  /** A key is absent when the record this sample came from did not carry it. */
  values: Partial<Record<ChartKey, number>>
}

/** What the sampler hands the view. */
export interface ChartData {
  samples: readonly Sample[]
  /** Simulated seconds covered by the samples held. */
  span: number
}

/**
 * The plots for a recorded episode, up to and including the playhead.
 *
 * A **pure function** of the source and the time, which is the whole point:
 * playing twice, scrubbing backwards, or simply re-rendering produces
 * identical points for the same playhead, because nothing is accumulated
 * anywhere. Every point is a recorded sample at its own recorded timestamp —
 * `frameAt`, never `sampleAt` — so no plotted value is interpolated.
 *
 * The bound is `frame.t <= t`, so a sample exactly at the playhead is in and
 * the series never runs ahead of the boat on screen.
 */
export function replayChartData(source: ReplaySource, t: number): ChartData {
  const last = source.indexAt(t)
  const samples: Sample[] = []
  for (let i = 0; i <= last; i += 1) {
    const frame = source.frameAt(i)
    if (frame === null || frame.t > t) {
      break
    }
    const record = diagnosticsFromFrame(frame)
    const values: Partial<Record<ChartKey, number>> = {}
    for (const series of CHART_SERIES) {
      const v = series.read(record)
      if (v !== undefined) {
        values[series.key] = v
      }
    }
    samples.push({ t: frame.t, values })
  }
  return {
    samples,
    span: samples.length < 2 ? 0 : samples[samples.length - 1].t - samples[0].t,
  }
}

/**
 * The sampler. Call once per render with the latest published record; it
 * decides, by simulated time, whether this one becomes a sample.
 */
export function useChartSampler(
  diagnostics: Diagnostics | null,
  sampleHz: number,
  live = true,
): ChartData {
  const buffer = useRef<RingBuffer<Sample> | null>(null)
  const lastSampleT = useRef(Number.NEGATIVE_INFINITY)
  const published = useRef<ChartData>({ samples: [], span: 0 })
  if (buffer.current === null) {
    buffer.current = new RingBuffer<Sample>(CHART_CAPACITY)
  }
  const rb = buffer.current
  let changed = false

  // Frozen while a recorded episode is being inspected: the live simulation is
  // paused, its `t` is not advancing, and appending to this buffer — or
  // clearing it, which a wound-back `t` would otherwise do — would let a
  // replay edit the live history it is supposed to leave alone (task 10.4).
  if (live && diagnostics !== null) {
    const interval = sampleHz > 0 ? 1 / sampleHz : 0
    // A reset winds `t` back to zero; so does loading a scenario. Start again
    // rather than drawing a line backwards through the chart.
    if (diagnostics.t < lastSampleT.current) {
      rb.clear()
      lastSampleT.current = Number.NEGATIVE_INFINITY
      changed = true
    }
    if (diagnostics.t - lastSampleT.current >= interval) {
      lastSampleT.current = diagnostics.t
      const values = {} as Record<ChartKey, number>
      for (const series of CHART_SERIES) {
        const v = series.read(diagnostics)
        if (v !== undefined) {
          values[series.key] = v
        }
      }
      rb.push({ t: diagnostics.t, values })
      changed = true
    }
  }

  // The identity is stable between samples on purpose: the app re-renders
  // every animation frame, the series only change at `sampleHz`, and
  // `Charts` is memoised on this object. Rebuilding it every frame would
  // redraw eight polylines of up to `CHART_CAPACITY` points each at 60 Hz to
  // show the same picture.
  if (changed) {
    const samples = rb.toArray()
    published.current = {
      samples,
      span: samples.length < 2 ? 0 : samples[samples.length - 1].t - samples[0].t,
    }
  }
  return published.current
}

const WIDTH = 300
const HEIGHT = 46

function polyline(
  samples: readonly Sample[],
  key: ChartKey,
): { points: string; lo: number; hi: number; recorded: number } {
  const present = samples.filter((s) => s.values[key] !== undefined)
  if (present.length === 0) {
    return { points: '', lo: 0, hi: 0, recorded: 0 }
  }
  let lo = Number.POSITIVE_INFINITY
  let hi = Number.NEGATIVE_INFINITY
  for (const s of present) {
    const v = s.values[key] as number
    if (Number.isFinite(v)) {
      lo = Math.min(lo, v)
      hi = Math.max(hi, v)
    }
  }
  if (!Number.isFinite(lo) || !Number.isFinite(hi)) {
    return { points: '', lo: 0, hi: 0, recorded: 0 }
  }
  // A flat series still needs a band, or every point lands on one row.
  const pad = hi - lo < 1e-9 ? Math.max(1e-9, Math.abs(hi) * 0.05 + 1e-6) : (hi - lo) * 0.08
  const low = lo - pad
  const high = hi + pad
  const t0 = present[0].t
  const t1 = present[present.length - 1].t
  const dt = t1 - t0 < 1e-9 ? 1 : t1 - t0
  const points = present
    .map((s) => {
      const x = ((s.t - t0) / dt) * WIDTH
      const y = HEIGHT - (((s.values[key] as number) - low) / (high - low)) * HEIGHT
      return `${x.toFixed(1)},${y.toFixed(1)}`
    })
    .join(' ')
  return { points, lo, hi, recorded: present.length }
}

export interface ChartsProps {
  data: ChartData
  enabled: Record<ChartKey, boolean>
  onToggle: (key: ChartKey, on: boolean) => void
  sampleHz: number
  onSampleHz: (hz: number) => void
  /** Which timeline the points belong to. */
  source: 'live' | 'recorded'
  /**
   * Hz, the resolution the points were taken at — the live sampler's rate, or
   * the episode's own `log_hz`. Displayed rather than left to be guessed: two
   * plots at different resolutions are not the same picture (task 10.4).
   */
  resolutionHz: number
}

/**
 * Memoised, and paired with the stable `ChartData` identity above: together
 * they mean the charts redraw once per *sample*, not once per frame.
 */
export const Charts = memo(function Charts({
  data,
  enabled,
  onToggle,
  sampleHz,
  onSampleHz,
  source,
  resolutionHz,
}: ChartsProps) {
  const active = CHART_KEYS.filter((k) => enabled[k])
  return (
    <section
      data-testid="charts"
      data-active={active.length}
      data-samples={data.samples.length}
      data-span={data.span}
      data-sample-hz={sampleHz}
      data-source={source}
      data-resolution-hz={resolutionHz}
      style={{ border: '1px solid #ccd', borderRadius: 4, padding: 8 }}
    >
      <div style={{ display: 'flex', gap: 8, alignItems: 'center', marginBottom: 6 }}>
        <strong style={{ flex: 1 }}>Charts</strong>
        <span data-testid="chart-resolution" style={{ color: '#667', fontSize: 12 }}>
          {source === 'recorded'
            ? `episode samples at ${resolutionHz} Hz`
            : `live at ${resolutionHz} Hz`}
        </span>
        <label style={{ display: 'flex', gap: 4, alignItems: 'center' }}>
          sample
          <select
            data-testid="chart-sample-hz"
            value={sampleHz}
            onChange={(e) => onSampleHz(Number(e.target.value))}
          >
            {SAMPLE_HZ_CHOICES.map((hz) => (
              <option key={hz} value={hz}>
                {hz} Hz
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          data-testid="charts-all-on"
          onClick={() => CHART_KEYS.forEach((k) => onToggle(k, true))}
        >
          All
        </button>
        <button
          type="button"
          data-testid="charts-all-off"
          onClick={() => CHART_KEYS.forEach((k) => onToggle(k, false))}
        >
          None
        </button>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '1px 8px', marginBottom: 6 }}>
        {CHART_SERIES.map((s) => (
          <label key={s.key} style={{ display: 'flex', gap: 5, alignItems: 'center' }}>
            <input
              type="checkbox"
              data-testid={`chart-toggle-${s.key}`}
              checked={enabled[s.key]}
              onChange={(e) => onToggle(s.key, e.target.checked)}
            />
            {s.label}
          </label>
        ))}
      </div>

      {active.map((key) => {
        const series = SERIES_BY_KEY[key]
        const { points, lo, hi, recorded } = polyline(data.samples, key)
        const withValue = data.samples.filter((s) => s.values[key] !== undefined)
        const newest = withValue.length === 0 ? null : withValue[withValue.length - 1]
        const latest = newest === null ? null : (newest.values[key] as number)
        // A series the episode does not record has no points at all. It is
        // labelled, not plotted as zero (RV59).
        const missing = recorded === 0 && data.samples.length > 0
        return (
          <figure
            key={key}
            data-testid={`chart-${key}`}
            data-points={recorded}
            data-recorded={missing ? 'false' : 'true'}
            data-latest={latest ?? ''}
            // The simulated time the newest point was taken at. Published so a
            // test can wait until the chart and the live readout describe the
            // *same* state rather than comparing across a sample interval.
            data-latest-t={newest === null ? '' : newest.t}
            data-min={lo}
            data-max={hi}
            style={{ margin: '0 0 6px' }}
          >
            <figcaption style={{ display: 'flex', gap: 6, fontSize: 12 }}>
              <span style={{ flex: 1, color: series.colour }}>{series.label}</span>
              <span
                style={{
                  fontVariantNumeric: 'tabular-nums',
                  color: missing ? '#889' : undefined,
                }}
              >
                {missing
                  ? NOT_RECORDED
                  : `${(latest ?? 0).toFixed(3)} ${series.unit}`}
              </span>
            </figcaption>
            <svg
              width={WIDTH}
              height={HEIGHT}
              viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
              style={{ display: 'block', background: '#f7f9fb' }}
            >
              <polyline points={points} fill="none" stroke={series.colour} strokeWidth={1.5} />
            </svg>
          </figure>
        )
      })}
    </section>
  )
})
