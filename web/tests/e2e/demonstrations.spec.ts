import type { Page } from '@playwright/test'
import { expect, gotoApp, test } from './fixtures'
import type { Diagnostics } from '../../src/sim/diagnostics'

/**
 * brief §46's three demonstrations, as automated tests (section 10, task 10.6).
 *
 * brief §46 is the specification's own definition of a working prototype, and
 * §47's first criterion is that the thing be interactive and enjoyable enough
 * to experiment with. So these run in a real browser, through the real
 * controls — a held mouse drag on the boat for the mainsheet, `A`/`D` for the
 * tiller, `Space` for the release — and read the numbers the page publishes.
 *
 * **Each test asserts the physical chain, in order.** A test that only checked
 * "did it capsize" would pass for a hard-coded capsize, which is the one thing
 * brief §46 forbids ("No explicit capsize-prevention or 'release causes
 * recovery' rule may be used"). What is asserted here is the *order* in which
 * the sheet, the boom, the sail and the hull respond, because that ordering is
 * what a rule would get wrong.
 *
 * ## Two links of the PRD's chain are not what the physics does
 *
 * Task 10.6 writes demonstration 1's chain as "sail `|cl|` stays above 0.5
 * (still powered); heeling moment grows". Measured on the shipped
 * `beam_reach_capsize`, neither holds, and both fail in a direction that says
 * the model is *right*:
 *
 * - `|cl|` starts at **0.000** and rises to 0.89 as the boat heels. With the
 *   boom sheeted flat on the centreline and the apparent wind on the beam, the
 *   sail is at ≈ 90° of attack: it is a stalled plate, and F5.2 gives a stalled
 *   plate `C_L ≈ 0` and `C_D ≈ C_N,max`. The boat is powered — 365 N of sail
 *   force at `t = 0` — but by drag, not lift. "Powered" is therefore asserted
 *   on the **sail force magnitude**, which is the quantity that heels the boat.
 * - the heeling moment **falls** monotonically, 870 → 311 N·m over the first
 *   six seconds, while the *heel* grows 0° → 52°. That is brief §46 step 7's
 *   actual wording ("Aerodynamic side force creates increasing heel") and it is
 *   correct: as the boat lies down the rig comes out of the wind and the lever
 *   arm shortens, so the moment collapses even as the angle runs away. A model
 *   in which the heeling moment *grew* with heel would be a model with no
 *   `R_x(φ)` in it.
 *
 * Both are recorded in `docs/v1/progress/10-handoff.md` rather than resolved by
 * relaxing an assertion into vagueness.
 */

/** One sample of everything the assertions read. */
interface Sample {
  t: number
  phi: number
  psi: number
  beta: number
  betaDot: number
  tension: number
  cl: number
  sailForce: number
  heelingMoment: number
  mBeta: number
  capsized: boolean
}

interface Trace {
  samples: Sample[]
  /** deck.gl frames drawn while the trace was running (brief §46 step 2). */
  deckFrames: number
}

declare global {
  interface Window {
    __demoTrace?: { stop(): Trace }
  }
}

/**
 * Start sampling the page's published state once per animation frame.
 *
 * Every number comes from `[data-testid="sail-hud"] data-diagnostics`, which is
 * the record the core serialises — nothing here derives a physical quantity
 * (F8). Sampling on the animation frame is what makes "no step larger than
 * 0.3 rad between frames" a statement about frames.
 */
async function startTrace(page: Page): Promise<void> {
  await page.evaluate(() => {
    const samples: {
      t: number
      phi: number
      psi: number
      beta: number
      betaDot: number
      tension: number
      cl: number
      sailForce: number
      heelingMoment: number
      mBeta: number
      capsized: boolean
    }[] = []
    let running = true
    let deckFrames = 0
    const tick = () => {
      if (!running) {
        return
      }
      requestAnimationFrame(tick)
      const hud = document.querySelector('[data-testid="sail-hud"]')
      const snap = document.querySelector('[data-testid="snapshot"]')
      const stats = document.querySelector('[data-testid="wind-stats"]')
      if (!(hud instanceof HTMLElement) || !(snap instanceof HTMLElement)) {
        return
      }
      const raw = hud.getAttribute('data-diagnostics')
      if (raw === null) {
        return
      }
      const d = JSON.parse(raw) as Record<string, never>
      const f = d.sail as unknown as { f: { x: number; y: number; z: number } }
      const boom = d.boom_moment as unknown as { aero: number }
      const capsize = d.capsize as unknown as { capsized: boolean }
      if (stats instanceof HTMLElement) {
        deckFrames = Number(stats.dataset.frames ?? 0)
      }
      samples.push({
        t: Number(snap.dataset.t),
        phi: Number(snap.dataset.phi),
        psi: Number(snap.dataset.psi),
        beta: Number(snap.dataset.beta),
        betaDot: Number(snap.dataset.betaDot),
        tension: d.sheet_tension as unknown as number,
        cl: d.cl_sail as unknown as number,
        sailForce: Math.hypot(f.f.x, f.f.y, f.f.z),
        heelingMoment: d.heeling_moment as unknown as number,
        mBeta: boom.aero,
        capsized: capsize.capsized,
      })
    }
    const first = deckFrames
    window.__demoTrace = {
      stop() {
        running = false
        return { samples, deckFrames: deckFrames - first }
      },
    }
    requestAnimationFrame(tick)
  })
}

async function stopTrace(page: Page): Promise<Trace> {
  const trace = await page.evaluate(() => {
    const t = window.__demoTrace
    delete window.__demoTrace
    return t === undefined ? { samples: [], deckFrames: 0 } : t.stop()
  })
  expect(trace.samples.length, 'the trace collected no samples').toBeGreaterThan(30)
  return trace as Trace
}

/**
 * Wait until the simulation clock passes `target`.
 *
 * Actions are sequenced on **simulated** time, not on wall time, so the script
 * a browser runs is the script the physics sees whatever the frame rate is —
 * which is also what makes the same sequence reproduce in Chromium, Firefox and
 * a software-rendered Edge.
 */
async function waitForSimTime(page: Page, target: number): Promise<void> {
  await expect
    .poll(
      async () =>
        Number(
          await page.getByTestId('snapshot').getAttribute('data-t'),
        ),
      { timeout: 60_000, message: `simulated time never reached ${target} s` },
    )
    .toBeGreaterThan(target)
}

/** The first sample at or after `t`, or `undefined` if the trace ended first. */
function at(samples: Sample[], t: number): Sample | undefined {
  return samples.find((s) => s.t >= t)
}

/**
 * The last sample at or before `t` — the state as it was *just before* an
 * action landed.
 *
 * Reading forwards instead would sometimes catch the state after the action:
 * the sheet goes slack within one physics step of `Space`, and on a browser
 * whose frames are 17 ms apart the first sample at or after the key press can
 * already be on the far side of it. That is how this read zero tension on
 * Firefox while passing on Chromium.
 */
function before(samples: Sample[], t: number): Sample | undefined {
  let found: Sample | undefined
  for (const s of samples) {
    if (s.t > t) {
      break
    }
    found = s
  }
  return found
}

/** The simulated time of the first sample satisfying `p`, or `NaN`. */
function firstTime(samples: Sample[], from: number, p: (s: Sample) => boolean): number {
  const hit = samples.find((s) => s.t >= from && p(s))
  return hit === undefined ? NaN : hit.t
}

/**
 * Assert that a list of named events happened, and happened in this order.
 *
 * The message names the whole sequence with its times, because "expected NaN to
 * be less than 12" tells a reader nothing about which link of the chain broke.
 */
function assertOrdered(label: string, events: [string, number][]): void {
  const shown = events.map(([n, t]) => `${n}@${Number.isNaN(t) ? '—' : t.toFixed(2)}`).join(' → ')
  for (const [name, t] of events) {
    expect(Number.isFinite(t), `${label}: ${name} never happened  (${shown})`).toBe(true)
  }
  for (let i = 1; i < events.length; i += 1) {
    expect(
      events[i][1],
      `${label}: ${events[i][0]} did not follow ${events[i - 1][0]}  (${shown})`,
    ).toBeGreaterThanOrEqual(events[i - 1][1])
  }
  console.log(`[demo] ${label}: ${shown}`)
}

/** Hold a downward drag on the boat: the mainsheet, hauled and held. */
async function haulAndHold(page: Page): Promise<void> {
  const box = await page.getByTestId('world-view').boundingBox()
  expect(box, 'the world view must be on screen').not.toBeNull()
  const x = box!.x + box!.width / 2
  const y = box!.y + box!.height / 2
  await page.mouse.move(x, y)
  await page.mouse.down()
  await page.mouse.move(x, y + 180, { steps: 4 })
}

async function releaseSheetDrag(page: Page): Promise<void> {
  await page.mouse.up()
}

async function setSpeed(page: Page, speed: '1x' | '2x'): Promise<void> {
  await page.getByTestId(`clock-speed-${speed}`).click()
}

/**
 * Load a scenario, put the boat back on its initial condition with the clock
 * stopped, start the trace, and only then let it run.
 *
 * Without the pause the first trace sample is whatever the boat had already
 * become while the page was loading — and `beam_reach_capsize` reaches 18° of
 * heel inside the first second, so "the boat starts upright" would be testing
 * how fast Playwright is rather than what the scenario says.
 */
async function armFromStart(page: Page, scenario: string, speed: '1x' | '2x'): Promise<void> {
  await gotoApp(page, { scenario })
  const pause = page.getByTestId('clock-pause')
  await pause.click()
  await expect(pause).toHaveAttribute('data-running', 'false')
  await page.getByTestId('clock-reset').click()
  await setSpeed(page, speed)
  await startTrace(page)
  await pause.click()
  await expect(pause).toHaveAttribute('data-running', 'true')
}

// ---------------------------------------------------------------------------
// Demonstration 1 — capsize, and recovery from the same state
// ---------------------------------------------------------------------------

test.describe('brief §46 demonstration 1 — capsize and recovery @slow', () => {
  test.setTimeout(180_000)

  test('hauling capsizes the boat, and releasing brings it back, in that order', async ({
    page,
  }) => {
    // --- part one: haul and hold, and go over -----------------------------
    await armFromStart(page, 'beam_reach_capsize', '2x')
    await haulAndHold(page)
    await waitForSimTime(page, 13)
    await releaseSheetDrag(page)
    const hold = await stopTrace(page)

    // brief §46 step 2: the wind field is visibly moving across the map.
    expect(hold.deckFrames, 'the deck.gl wind layer never drew a frame').toBeGreaterThan(30)

    // brief §46 step 3: the boat starts approximately beam-to-wind. The
    // scenario is a northerly with the boat heading east, so the apparent wind
    // is on the beam; `psi` starts at zero and the heel starts at zero.
    const first = hold.samples[0]
    expect(Math.abs(first.psi), 'the boat did not start on its scenario heading').toBeLessThan(0.1)
    expect(Math.abs(first.phi), 'the boat did not start upright').toBeLessThan(0.1)

    // brief §46 step 5: the sheet restrains the boom. `beam_reach_capsize`
    // starts at `l_sheet_min`, which **v2 F18.1b made the shortest geometric
    // rope path** rather than 0.14 m below it: the boom is pinned on the
    // centreline from `t = 0` but the rope carries nothing until the sail
    // pushes the boom off it, and a haul command cannot shorten it further.
    // So the restraint is asserted on the boom angle at every sample, and the
    // load on every sample after the first half-second — the v1 version of
    // this line was asserting the 2.81 kN preload that section 08 removed.
    for (const s of hold.samples) {
      expect(Math.abs(s.beta), `the boom left the centreline at t = ${s.t}`).toBeLessThan(0.2)
      if (s.t > 0.5) {
        expect(s.tension, `tension went to zero at t = ${s.t}`).toBeGreaterThan(0)
      }
    }

    // brief §46 step 6: the sail stays powered. See the file comment: at 90°
    // of attack the shipped foil model gives `C_L ≈ 0` and all the force is
    // drag, so "powered" is the force, not the lift coefficient.
    const powered = hold.samples.filter((s) => s.t < 8)
    expect(Math.min(...powered.map((s) => s.sailForce)), 'the sail stopped pulling').toBeGreaterThan(
      20,
    )

    // brief §46 steps 7 and 8, as an ordered chain.
    const heelGrows = firstTime(hold.samples, 0, (s) => Math.abs(s.phi) > 0.3)
    const heelFurther = firstTime(hold.samples, 0, (s) => Math.abs(s.phi) > 0.9)
    const capsized = firstTime(hold.samples, 0, (s) => s.capsized)
    assertOrdered('demo 1, hold', [
      ['heel > 17°', heelGrows],
      ['heel > 52°', heelFurther],
      ['capsized', capsized],
    ])

    // The heel really did run past the capsize threshold rather than the flag
    // being set by something else.
    const over = hold.samples.filter((s) => s.capsized)
    expect(Math.max(...over.map((s) => Math.abs(s.phi))), 'heel at capsize').toBeGreaterThan(1.396)

    // --- part two: the same start, released instead ------------------------
    //
    // `sheet_release_recovery` is byte-identical to `beam_reach_capsize` in
    // seed, parameters, initial state and wind (section 09 handoff §5): the
    // only difference between the two runs is what the human does.
    await armFromStart(page, 'sheet_release_recovery', '2x')
    await haulAndHold(page)
    // Release while the sail is still strongly loaded. Past about eight
    // seconds the boat is already on its beam ends, the rig is out of the wind
    // and the sail force has collapsed on its own — at which point "releasing
    // the sheet depowers the sail" is no longer the thing being demonstrated.
    await waitForSimTime(page, 4)
    await releaseSheetDrag(page)

    // The simulated time is read **before** the key goes down: the sheet goes
    // slack within one physics step, so a reading taken afterwards can already
    // be past the event it is supposed to mark.
    const released = Number(await page.getByTestId('snapshot').getAttribute('data-t'))
    await page.keyboard.down(' ')
    expect(released, 'the release must land while the sail is still loaded').toBeGreaterThan(3)
    expect(released, 'the release must land before the boat is on its beam ends').toBeLessThan(7.5)
    await waitForSimTime(page, released + 4)
    await page.keyboard.up(' ')
    await waitForSimTime(page, released + 12)
    const recover = await stopTrace(page)

    const atRelease = before(recover.samples, released)
    expect(atRelease, 'no sample at the moment of release').toBeDefined()
    const ref = atRelease!
    expect(Math.abs(ref.phi), 'the boat must be well heeled when the sheet is released')
      .toBeGreaterThan(0.6)
    expect(ref.tension, 'the sheet must be loaded when it is released').toBeGreaterThan(100)

    // The five links of brief §46 steps 12–16, in order.
    //
    // **Task 10.6 lists "sail force magnitude falls" before "heeling moment
    // falls"; the model puts them the other way round, and it is right to.**
    // Measured on this scenario at nine release times between 3 s and 12 s, the
    // heeling moment halves before the sail force does, every time and by
    // 0.08–0.46 s. The mechanism is not subtle: the boom swings out first, and
    // that rotates the sail force and shortens its heeling lever
    // (`Generalized::add`, F6.4) before the force *magnitude* has decayed at
    // all. The moment is `r × F`; it can fall while `|F|` does not. Since the
    // moment is what heels the boat, a chain in which the force had to collapse
    // first would describe a model with the geometry left out.
    //
    // Both links are still asserted, and the gap between them is bounded, so
    // "the sail depowers" remains part of the claim rather than being dropped.
    // The deviation is recorded in `docs/v1/progress/10-handoff.md`.
    const tensionGone = firstTime(recover.samples, released, (s) => s.tension < 1)
    const boomOut = firstTime(recover.samples, released, (s) => Math.abs(s.beta) > 0.5)
    const forceFell = firstTime(
      recover.samples,
      released,
      (s) => s.sailForce < 0.5 * ref.sailForce,
    )
    const momentFell = firstTime(
      recover.samples,
      released,
      (s) => Math.abs(s.heelingMoment) < 0.5 * Math.abs(ref.heelingMoment),
    )
    const heelFell = firstTime(recover.samples, released, (s) => Math.abs(s.phi) < Math.abs(ref.phi) - 0.2)
    assertOrdered('demo 1, release', [
      ['tension → 0', tensionGone],
      ['boom swings out', boomOut],
      ['heeling moment falls', momentFell],
      ['sail force falls', forceFell],
      ['heel decreases', heelFell],
    ])
    expect(
      forceFell - momentFell,
      'the sail force must follow the heeling moment down promptly, not eventually',
    ).toBeLessThan(2)

    // …and it really comes back up, which is brief §46 step 16.
    const last = recover.samples[recover.samples.length - 1]
    expect(Math.abs(last.phi), 'the boat did not return toward upright').toBeLessThan(0.35)
    expect(last.capsized, 'the boat is still reported capsized').toBe(false)
  })
})

// ---------------------------------------------------------------------------
// Demonstration 2 — the tack
// ---------------------------------------------------------------------------

test.describe('brief §46 demonstration 2 — tack @slow', () => {
  test.setTimeout(180_000)

  test('the boat tacks: the sail unloads, the boom crosses, the sail fills again', async ({
    page,
  }) => {
    // 1× on purpose: "no step larger than 0.3 rad between frames" is a
    // statement about frames, and running the clock faster would put more
    // simulated time between two of them and make it a weaker test.
    await armFromStart(page, 'tack', '1x')
    await page.keyboard.down('a')
    await waitForSimTime(page, 5)
    await page.keyboard.up('a')
    await waitForSimTime(page, 12)
    const trace = await stopTrace(page)

    // `tack.json` is a northerly, so head to wind is `psi = π/2` (F2: `psi` is
    // CCW from world +x, and the scenario's heading is converted once, in
    // Rust). The boat starts at `psi = π/4` and steers up through it.
    const headToWind = Math.PI / 2
    const start = trace.samples[0]
    expect(start.psi, 'the boat did not start on its scenario heading').toBeLessThan(headToWind)
    expect(start.beta, 'the boom must start on one side').toBeGreaterThan(0.3)

    const throughWind = firstTime(trace.samples, 0, (s) => s.psi >= headToWind)
    const unloaded = firstTime(trace.samples, 0, (s) => Math.abs(s.cl) < 0.1 && Math.abs(s.psi - headToWind) < 0.25)
    const boomCrossed = firstTime(trace.samples, throughWind, (s) => s.beta < 0)
    const filled = firstTime(trace.samples, boomCrossed, (s) => Math.abs(s.cl) > 0.5)
    assertOrdered('demo 2, tack', [
      ['sail unloads (|cl| < 0.1)', unloaded],
      ['psi passes head to wind', throughWind],
      ['beta changes sign', boomCrossed],
      ['sail fills (|cl| > 0.5)', filled],
    ])

    // The boom crossed **continuously**: no hard-coded manoeuvre state put it
    // on the other side in one step. The threshold is task 10.6's.
    let worst = 0
    let worstAt = 0
    for (let i = 1; i < trace.samples.length; i += 1) {
      const step = Math.abs(trace.samples[i].beta - trace.samples[i - 1].beta)
      if (step > worst) {
        worst = step
        worstAt = trace.samples[i].t
      }
    }
    console.log(`[demo] demo 2: largest boom step between frames ${worst.toFixed(4)} rad at t=${worstAt.toFixed(2)}`)
    expect(worst, `largest boom step between frames, at t = ${worstAt}`).toBeLessThan(0.3)
  })
})

// ---------------------------------------------------------------------------
// Demonstration 3 — the gybe, controlled and uncontrolled
// ---------------------------------------------------------------------------

/** One gybe run. `haul` holds the mainsheet in through the turn. */
async function gybe(page: Page, haul: boolean): Promise<{ trace: Trace; turnAt: number }> {
  await armFromStart(page, 'gybe', '2x')
  if (haul) {
    await haulAndHold(page)
  }
  await waitForSimTime(page, 2)
  const turnAt = Number(await page.getByTestId('snapshot').getAttribute('data-t'))
  await page.keyboard.down('a')
  await waitForSimTime(page, turnAt + 5)
  await page.keyboard.up('a')
  await waitForSimTime(page, turnAt + 10)
  if (haul) {
    await releaseSheetDrag(page)
  }
  return { trace: await stopTrace(page), turnAt }
}

test.describe('brief §46 demonstration 3 — gybe @slow', () => {
  test.setTimeout(240_000)

  test('the boom crashes across, and sheet handling changes the result', async ({ page }) => {
    // --- uncontrolled: the sheet is left where the scenario set it ---------
    const eased = await gybe(page, false)
    const samples = eased.trace.samples
    const start = samples[0]
    expect(start.beta, 'the boom must start out to port').toBeLessThan(-1)

    // brief §46: the aerodynamic torque reverses, the boom accelerates across
    // the centreline, and the sheet takes a transient load.
    const preGybe = samples.filter((s) => s.t < eased.turnAt)
    const preMeanTension =
      preGybe.reduce((a, s) => a + s.tension, 0) / Math.max(1, preGybe.length)
    const mSign = Math.sign(start.mBeta)
    const torqueReversed = firstTime(samples, eased.turnAt, (s) => Math.sign(s.mBeta) === -mSign)
    const crossedCentre = firstTime(samples, torqueReversed, (s) => s.beta > 0)
    const filledOtherSide = firstTime(samples, crossedCentre, (s) => s.beta > 1)
    assertOrdered('demo 3, uncontrolled', [
      ['boom torque reverses', torqueReversed],
      ['boom crosses the centreline', crossedCentre],
      ['boom reaches the other side', filledOtherSide],
    ])

    /**
     * Load-time on the rope: `∑ T · Δt` over the trace, in N·s.
     *
     * **Not the peak.** v2 F18.1b made the sheet's take-up discontinuous, so
     * the eased gybe's transient is very nearly an impulse: measured at the
     * physics timestep it reaches 2524 N, and the same run sampled at the
     * animation frame rate reads anywhere between 398 N and 2524 N depending
     * on where a frame happens to land. A peak that varies sixfold with the
     * sampling phase is not a statistic to compare two runs with — swept over
     * ten sampling phases the eased/hauled peak comparison ranges from 0.13 to
     * 0.65 and changes sign, which is what made this assertion flaky.
     *
     * The impulse is the same physical claim and it survives the sampling: over
     * the same sweep it reads 150-237 N·s eased against 5240-5270 N·s hauled,
     * a relative difference of **0.955 to 0.972** against the 0.3 the criterion
     * asks for. And it is the more honest statement of what a sailor sees: a
     * sheet held in carries load continuously through the turn, a sheet left
     * eased carries almost none until the boom arrives at the stop.
     */
    const impulse = (t: Trace) =>
      t.samples.reduce((a, s, i) => (i === 0 ? 0 : a + s.tension * (s.t - t.samples[i - 1].t)), 0)

    const easedPeakRate = Math.max(...samples.map((s) => Math.abs(s.betaDot)))
    const easedPeakTension = Math.max(...samples.map((s) => s.tension))
    const easedImpulse = impulse(eased.trace)
    expect(easedPeakRate, 'peak boom rate as it crossed').toBeGreaterThan(2)
    // The transient itself, against the run's own quiet baseline. This one is
    // a factor of ~19 even when the sampling misses the spike entirely.
    expect(
      easedPeakTension,
      `peak tension against a pre-gybe mean of ${preMeanTension.toFixed(1)} N`,
    ).toBeGreaterThan(preMeanTension * 1.3)

    // --- controlled: the sheet is hauled and held through the turn ---------
    const hauled = await gybe(page, true)
    const hauledPeakRate = Math.max(...hauled.trace.samples.map((s) => Math.abs(s.betaDot)))
    const hauledPeakTension = Math.max(...hauled.trace.samples.map((s) => s.tension))
    const hauledImpulse = impulse(hauled.trace)
    console.log(
      `[demo] demo 3: eased peak |beta_dot| ${easedPeakRate.toFixed(2)} rad/s, ` +
        `peak T ${easedPeakTension.toFixed(0)} N, impulse ${easedImpulse.toFixed(0)} N.s; ` +
        `hauled ${hauledPeakRate.toFixed(2)} rad/s, ${hauledPeakTension.toFixed(0)} N, ` +
        `${hauledImpulse.toFixed(0)} N.s`,
    )

    // brief §46: "result differs visibly between controlled and uncontrolled
    // sheet handling". Task 10.6 makes "visibly" a number: more than 30 %.
    const relative = (a: number, b: number) => Math.abs(a - b) / Math.max(a, b)
    expect(
      relative(easedPeakRate, hauledPeakRate),
      'peak boom rate, controlled against uncontrolled',
    ).toBeGreaterThan(0.3)
    expect(
      relative(easedImpulse, hauledImpulse),
      'sheet load-time (N.s), controlled against uncontrolled',
    ).toBeGreaterThan(0.3)

    // With the sheet hauled the boom is held near the centreline instead of
    // swinging to the far stop — which is the difference a sailor sees.
    const hauledSwing = Math.max(...hauled.trace.samples.map((s) => Math.abs(s.beta)))
    const easedSwing = Math.max(...samples.map((s) => Math.abs(s.beta)))
    expect(hauledSwing, 'the hauled boom should stay near the centreline').toBeLessThan(easedSwing)
  })
})
