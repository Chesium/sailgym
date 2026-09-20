import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'

import { visualDims } from '../../src/render/visualDims'
import { ilcaParams } from './ilca'

/**
 * The drawing dimensions of v2 section 01, task 1.1.
 *
 * Two of these are about the *discipline* rather than the geometry: that every
 * visual constant says where it came from, and that none of them is a metre
 * value hiding in the render layer (normative delta D2).
 */
describe('visualDims', () => {
  it('sail_triangle_has_the_parameter_area', () => {
    // The drawn sail is tack → head → clew, tack and clew at `z_boom` and the
    // head at the masthead, so its area is ½ · luff · foot. `mastHeight` is
    // *defined* by setting that equal to `sail.area`; this is the check that
    // the definition was not quietly replaced by a constant.
    for (const overrides of [
      {},
      { sail: { area: 5.0, boom_length: 3.4 } },
    ]) {
      const p = ilcaParams(overrides)
      const d = visualDims(p)
      const drawn = 0.5 * (d.mastHeight - p.sheet.z_boom) * p.sail.boom_length
      expect(Math.abs(drawn - p.sail.area)).toBeLessThan(1e-12)
    }
  })

  it('dims_scale_with_parameters', () => {
    const one = visualDims(ilcaParams())
    const two = visualDims(ilcaParams({ hull: { beam: 2 * ilcaParams().hull.beam } }))

    // The topside height is exactly proportional to beam: no hidden constant.
    expect(two.chineZ - two.deckZ).toBe(2 * (one.chineZ - one.deckZ))

    // The canoe depth likewise, to within one unit in the last place. It is
    // not bit-exact for the same reason `0.1 + 0.2 !== 0.3`: `keelZ` is formed
    // by subtracting the depth from `chineZ`, whose exponent is one larger, so
    // forming `keelZ` rounds. The slack allowed is one unit in the last place
    // of `keelZ` itself — exactly where that rounding happened — and not a
    // tolerance chosen to make the test pass. The products the depths are
    // built from double exactly, which the assertions below pin down.
    const depthOne = one.chineZ - one.keelZ
    const depthTwo = two.chineZ - two.keelZ
    expect(Math.abs(depthTwo - 2 * depthOne)).toBeLessThanOrEqual(
      Number.EPSILON * Math.abs(two.keelZ),
    )

    // Every other length that is a ratio of beam or LOA scales exactly.
    expect(two.rockerRise).toBe(2 * one.rockerRise)
    expect(two.sheerRise).toBe(one.sheerRise)
    expect(two.deckZ).toBe(one.deckZ)

    const long = visualDims(ilcaParams({ hull: { loa: 2 * ilcaParams().hull.loa } }))
    expect(long.sheerRise).toBe(2 * one.sheerRise)
    expect(long.boardChord).toBe(2 * one.boardChord)
    expect(long.rudderChord).toBe(2 * one.rudderChord)
    // Span follows from area / chord, so a longer boat gets a shallower board.
    expect(long.boardChord * long.boardSpan).toBeCloseTo(ilcaParams().board.area, 12)
    expect(long.rudderChord * long.rudderSpan).toBeCloseTo(ilcaParams().rudder.area, 12)
  })

  it('every_visual_constant_has_provenance', () => {
    const source = readFileSync('src/render/visualDims.ts', 'utf8')
    const lines = source.split('\n')
    const exported: string[] = []
    const documented: string[] = []

    lines.forEach((line, i) => {
      const match = /^export const (\w+)/.exec(line)
      if (match === null) {
        return
      }
      exported.push(match[1])
      // Walk back over the comment block immediately above the declaration.
      const block: string[] = []
      for (let k = i - 1; k >= 0; k -= 1) {
        const above = lines[k].trim()
        if (above === '') {
          break
        }
        if (!(above.startsWith('*') || above.startsWith('/*') || above.startsWith('//'))) {
          break
        }
        block.unshift(above)
      }
      const text = block.join(' ')
      if (/VISUAL|derived \(F7\)|derived/.test(text)) {
        documented.push(match[1])
      }
    })

    expect(exported.length).toBeGreaterThan(0)
    expect(documented).toEqual(exported)
  })

  it('no_metre_value_is_hard_coded_under_render', () => {
    // The section 02 rule, extended by D2 to the new files. The literals are
    // the ILCA defaults for LOA, LWL, beam, boom length, sail area, CE height
    // and mast height.
    const forbidden = /4\.23|3\.81|1\.37|2\.72|7\.06|2\.40|5\.89/
    for (const file of [
      'src/render/model3d.ts',
      'src/render/visualDims.ts',
      'src/render/hullSolid.ts',
      'src/render/rigSolid.ts',
      'src/render/project3d.ts',
      'src/render/SailShape.ts',
      'src/render/BoatSvg.tsx',
      'src/render/BoatProbe.tsx',
      'src/render/SheetRope.tsx',
      'src/render/geometry.ts',
    ]) {
      expect(forbidden.test(readFileSync(file, 'utf8')), file).toBe(false)
    }
  })
})
