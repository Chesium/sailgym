import { describe, expect, it } from 'vitest'

import { PROBE_BETA, PROBE_DELTA_R, BOAT_PROBE_DEGREES } from '../../src/render/BoatProbe'
import { hullSolid } from '../../src/render/hullSolid'
import type { BoatModel, Vec2 } from '../../src/render/model3d'
import {
  containsPoint,
  depthAt,
  isTri,
  projectedArea,
  projectModel,
  type DrawItem,
  type DrawTri,
} from '../../src/render/project3d'
import { boatModel } from '../../src/render/rigSolid'
import { visualDims } from '../../src/render/visualDims'
import { ilcaParams } from './ilca'

/**
 * The thing no single module owns: **that the picture is right**, measured
 * against an independent reference rather than looked at (v2 section 01,
 * task 1.6).
 *
 * The reference is a sampled ray cast. Along each ray the *true* topmost
 * surface is the one whose plane is highest there — `depthAt` — and the
 * painter's algorithm's answer is whichever drawn item comes last in the draw
 * order. Where the two disagree, the painter's algorithm has mis-ordered two
 * faces, and the fraction of rays where they agree is exactly its error rate.
 */

const params = ilcaParams()
const dims = visualDims(params)
const hullTris = hullSolid(params.hull, dims)

function model(beta = PROBE_BETA, deltaR = PROBE_DELTA_R, alpha = 0): BoatModel {
  return boatModel(params, params.hull, dims, { beta, deltaR, alpha }, hullTris)
}

/** The angles the browser probes are drawn at; the same set, and the same rig. */
const PROBE_ANGLES = BOAT_PROBE_DEGREES.map((d) => (d * Math.PI) / 180)

/** Van der Corput radical inverse, for a low-discrepancy sample of the box. */
function radicalInverse(i: number, base: number): number {
  let f = 1
  let r = 0
  let n = i
  while (n > 0) {
    f /= base
    r += f * (n % base)
    n = Math.floor(n / base)
  }
  return r
}

const SAMPLES = 4000

function samplePoints(items: readonly DrawItem[]): Vec2[] {
  const tris = items.filter(isTri)
  const xs = tris.flatMap((t) => t.points.map((p) => p.x))
  const ys = tris.flatMap((t) => t.points.map((p) => p.y))
  const x0 = Math.min(...xs)
  const x1 = Math.max(...xs)
  const y0 = Math.min(...ys)
  const y1 = Math.max(...ys)
  return Array.from({ length: SAMPLES }, (_, i) => ({
    x: x0 + (x1 - x0) * radicalInverse(i + 1, 2),
    y: y0 + (y1 - y0) * radicalInverse(i + 1, 3),
  }))
}

/** Agreement between the painter's order and the ray reference, in [0, 1]. */
function agreement(phi: number): { rate: number; rays: number } {
  const items = projectModel(model(), phi)
  const tris = items
    .map((item, order) => ({ item, order }))
    .filter((o): o is { item: DrawTri; order: number } => isTri(o.item))
  let rays = 0
  let agree = 0
  for (const q of samplePoints(items)) {
    let painter = -1
    let reference = -1
    let best = -Infinity
    for (const { item, order } of tris) {
      if (!containsPoint(item, q)) {
        continue
      }
      const z = depthAt(item, q)
      if (Number.isNaN(z)) {
        // A triangle seen exactly edge-on covers no area along this ray.
        continue
      }
      painter = order
      if (z > best) {
        best = z
        reference = order
      }
    }
    if (painter < 0) {
      continue
    }
    rays += 1
    if (painter === reference) {
      agree += 1
    }
  }
  return { rate: rays === 0 ? 1 : agree / rays, rays }
}

describe('boat3d', () => {
  it('painter_matches_the_ray_reference', () => {
    const measured: string[] = []
    for (const degrees of BOAT_PROBE_DEGREES) {
      const phi = (degrees * Math.PI) / 180
      const { rate, rays } = agreement(phi)
      measured.push(`${degrees}° ${(rate * 100).toFixed(2)}% (${rays} rays)`)
      expect(rays, `${degrees}° produced no rays`).toBeGreaterThan(200)
      // Never weaken this. If it fails, subdivide the offending part further
      // in `B` — that is what the subdivision counts in task 1.3 are for.
      expect(rate, `${degrees}°`).toBeGreaterThanOrEqual(0.995)
      if (degrees === 0 || degrees === 180) {
        // Upright and inverted, the visible faces tile the plan-form with no
        // overlap at all, so the ordering is exact by construction.
        expect(rate, `${degrees}° must be exact`).toBe(1)
      }
    }
    // Printed so the figures land in the run log and in the handoff note.
    console.log(`[boat3d] painter vs ray reference — ${measured.join(', ')}`)
  })

  it('no_pop_across_a_full_roll', () => {
    // A face appearing or vanishing abruptly is the visual artefact this test
    // exists to catch. It cannot happen when the cull is by the sign of the
    // rolled normal's `z`: a face crosses that threshold exactly when its
    // projected area passes through zero.
    const m = model()
    const steps = 720
    const areas: number[] = []
    for (let i = 0; i <= steps; i += 1) {
      const phi = (2 * Math.PI * i) / steps
      areas.push(
        projectModel(m, phi)
          .filter(isTri)
          .reduce((s, t) => s + projectedArea(t), 0),
      )
    }
    const peak = Math.max(...areas)
    let worst = 0
    for (let i = 1; i < areas.length; i += 1) {
      worst = Math.max(worst, Math.abs(areas[i] - areas[i - 1]))
    }
    console.log(
      `[boat3d] largest area step across a full roll: ${((worst / peak) * 100).toFixed(3)} % of ${peak.toFixed(2)} m²`,
    )
    expect(worst / peak).toBeLessThan(0.02)
  })

  it('total_triangle_budget', () => {
    expect(model().tris.length).toBeLessThanOrEqual(144)
  })

  it('projection_is_deterministic', () => {
    const m = model()
    const first = projectModel(m, 0.73)
    for (let i = 0; i < 100; i += 1) {
      expect(projectModel(m, 0.73)).toEqual(first)
    }
  })

  it('survives_unwrapped_phi', () => {
    // F3: `φ` is never wrapped, so a boat that has rolled through inversion
    // and kept going presents 4 rad, not −2.28.
    for (const phi of [4.0, -4.0]) {
      const items = projectModel(model(), phi)
      expect(items.length).toBeGreaterThan(0)
      for (const item of items) {
        expect(Number.isFinite(item.depth)).toBe(true)
        if (isTri(item)) {
          for (const p of item.points) {
            expect(Number.isFinite(p.x) && Number.isFinite(p.y)).toBe(true)
          }
        } else {
          expect(Number.isFinite(item.a.x) && Number.isFinite(item.b.y)).toBe(true)
        }
      }
    }
  })

  it('the_probe_angles_and_the_reference_angles_are_the_same_set', () => {
    // The browser probes and this file must be measuring the same boat, or
    // "99.5 % at every probe angle" is a statement about a different picture.
    expect(PROBE_ANGLES.length).toBe(BOAT_PROBE_DEGREES.length)
    expect(BOAT_PROBE_DEGREES).toContain(0)
    expect(BOAT_PROBE_DEGREES).toContain(180)
  })
})
