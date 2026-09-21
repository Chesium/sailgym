/**
 * Everything the browser does with the wind, in one place.
 *
 * It owns the reusable grid buffer, the particle system, the screen-space
 * projections handed to deck.gl and the per-frame timings. It does **not**
 * own an animation frame: `useSimulation` has the only one in the application
 * and calls {@link WindFieldHandle.onFrame} from it (section 02 handoff,
 * item 6).
 *
 * All of it is bookkeeping around numbers Rust produced. There is no wind
 * model here — see `sampleGrid.ts`.
 *
 * ## It is the **live** field, and a replay does not call it
 *
 * Everything here samples the running `Sim`: the grid, the particles' drift
 * and `windAtBoat()`. None of it describes a recorded episode, so while one is
 * being inspected `App.tsx` does not call {@link WindFieldHandle.onFrame} at
 * all and does not draw the layers — the replay's wind comes from the episode
 * (`sim/replay.ts`'s `windFromFrame`), and the dense field is hidden with its
 * reason on the page. Sampling the live field behind a recorded boat is the
 * mixed timeline v2 section 10 exists to close (RV57), and the recorded vector
 * at the boat does not identify the whole field.
 */

import { useCallback, useRef } from 'react'

import type { Camera } from '../render/Camera'
import type { SimHandle } from '../sim/loadWasm'
import { readWindAtBoat, ZERO_WIND, type WindAtBoat } from '../ui/WindReadout'
import { DEFAULT_PARTICLES, ParticleSystem } from './particles'
import { projectInto } from './WindLayer'
import {
  ensureGrid,
  gridCallCount,
  sampleInto,
  type Bounds,
  type WindGrid,
} from './sampleGrid'

/**
 * Nodes in the live visualization grid.
 *
 * 4 096 is generous: the default field's shortest structure is
 * `length_scale / 3 = 40 m` across, which a typical viewport spans a handful
 * of times, so the grid resolves the field far beyond what the eye can see.
 * The 128×128 figure task 3.3 asks to be timed is measured separately by
 * {@link WindFieldHandle.benchmarkLargeGrid} rather than paid for every frame.
 */
const TARGET_CELLS = 4096

/** Nodes per axis in the timed benchmark grid (task 3.3). */
export const BENCHMARK_NODES = 128

/**
 * How far outside the viewport particles live, as a fraction of its size.
 *
 * Without it every trail would be born on screen, which reads as sparkling
 * rather than as wind blowing across the map.
 */
const BOUNDS_MARGIN = 0.15

/** Frame timings, for the HUD and for the acceptance record. */
export interface WindStats {
  frames: number
  /** `sample_wind_grid` calls since page load. */
  gridCalls: number
  /** Milliseconds in the last `sample_wind_grid` call. */
  gridMs: number
  /** Milliseconds of wind work in the last frame, sampling included. */
  windMs: number
  /** Wall milliseconds between the last two frames. */
  frameMs: number
  /** Milliseconds per `BENCHMARK_NODES`² call; `null` until measured. */
  benchmarkMs: number | null
}

export interface WindFieldHandle {
  onFrame(sim: SimHandle, camera: Camera, simTime: number): void
  /** The most recent grid, or `null` before the first frame. */
  grid(): WindGrid | null
  particles(): ParticleSystem
  /** Screen-space `[x, y]` pairs: trail heads and trail tails. */
  heads(): Float32Array
  tails(): Float32Array
  windAtBoat(): WindAtBoat
  stats(): WindStats
  /** Time a `BENCHMARK_NODES`² sample, in-browser. Records the median of 5. */
  benchmarkLargeGrid(sim: SimHandle): number
}

export function useWindField(): WindFieldHandle {
  const grid = useRef<WindGrid | null>(null)
  const particles = useRef(new ParticleSystem(DEFAULT_PARTICLES))
  const heads = useRef(new Float32Array(2 * DEFAULT_PARTICLES.count))
  const tails = useRef(new Float32Array(2 * DEFAULT_PARTICLES.count))
  const wind = useRef<WindAtBoat>(ZERO_WIND)
  const lastSimTime = useRef<number | null>(null)
  const lastFrameAt = useRef<number | null>(null)
  const benchmarkMs = useRef<number | null>(null)
  const stats = useRef<WindStats>({
    frames: 0,
    gridCalls: 0,
    gridMs: 0,
    windMs: 0,
    frameMs: 0,
    benchmarkMs: null,
  })

  const onFrame = useCallback((sim: SimHandle, camera: Camera, simTime: number) => {
    const now = performance.now()
    const started = now
    const bounds = visibleBounds(camera)

    const g = ensureGrid(grid.current, bounds, TARGET_CELLS)
    grid.current = g

    const beforeSample = performance.now()
    sampleInto(sim, g, simTime)
    const afterSample = performance.now()

    // Particles drift with *simulated* time, so they freeze when the clock is
    // paused and speed up at 4x — the visualization follows the simulation
    // rather than the wall clock (brief section 21).
    const previous = lastSimTime.current
    const dtSim = previous === null ? 0 : Math.max(0, simTime - previous)
    lastSimTime.current = simTime
    particles.current.step(g, dtSim, bounds)

    projectInto(particles.current.positions(), camera, heads.current)
    projectInto(particles.current.trailTails(), camera, tails.current)

    wind.current = readWindAtBoat(sim.wind_at_boat())

    const previousFrame = lastFrameAt.current
    lastFrameAt.current = now
    stats.current = {
      frames: stats.current.frames + 1,
      gridCalls: gridCallCount(),
      gridMs: afterSample - beforeSample,
      windMs: performance.now() - started,
      frameMs: previousFrame === null ? 0 : now - previousFrame,
      benchmarkMs: benchmarkMs.current,
    }
  }, [])

  const benchmarkLargeGrid = useCallback((sim: SimHandle): number => {
    const n = BENCHMARK_NODES
    const out = new Float32Array(2 * n * n)
    const runs: number[] = []
    for (let i = 0; i < 5; i += 1) {
      const t0 = performance.now()
      sim.sample_wind_grid(-500, -500, 1000 / (n - 1), 1000 / (n - 1), n, n, i, out)
      runs.push(performance.now() - t0)
    }
    runs.sort((a, b) => a - b)
    benchmarkMs.current = runs[2]
    stats.current = { ...stats.current, benchmarkMs: runs[2] }
    return runs[2]
  }, [])

  return {
    onFrame,
    grid: () => grid.current,
    particles: () => particles.current,
    heads: () => heads.current,
    tails: () => tails.current,
    windAtBoat: () => wind.current,
    stats: () => stats.current,
    benchmarkLargeGrid,
  }
}

/**
 * World rectangle covering the viewport, plus a margin.
 *
 * All four corners are unprojected, not just two: in `follow` mode the camera
 * is rotated, so the axis-aligned world bounds of a screen rectangle are not
 * the unprojection of its corners taken pairwise.
 */
export function visibleBounds(camera: Camera): Bounds {
  const { width, height } = camera.viewport
  const corners = [
    camera.screenToWorld({ x: 0, y: 0 }),
    camera.screenToWorld({ x: width, y: 0 }),
    camera.screenToWorld({ x: 0, y: height }),
    camera.screenToWorld({ x: width, y: height }),
  ]
  const xs = corners.map((c) => c.x)
  const ys = corners.map((c) => c.y)
  const minX = Math.min(...xs)
  const maxX = Math.max(...xs)
  const minY = Math.min(...ys)
  const maxY = Math.max(...ys)
  const mx = (maxX - minX) * BOUNDS_MARGIN
  const my = (maxY - minY) * BOUNDS_MARGIN
  return { minX: minX - mx, maxX: maxX + mx, minY: minY - my, maxY: maxY + my }
}

