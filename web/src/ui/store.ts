/**
 * UI state, and only UI state (F12: `zustand` for the interface, never for
 * physics).
 *
 * Nothing here is a simulation quantity. The mode, which overlays are on,
 * which series are charted, the vector scale and the sample rate are all
 * properties of the *view*; the boat's state lives in Rust and reaches React
 * through `useSimulation`. Keeping the two apart is what lets the whole panel
 * be torn down and rebuilt without touching a trajectory.
 *
 * The store is persisted to `localStorage`, so the mode and the toggles
 * survive a reload (task 8.2). Persistence is best-effort: a browser with
 * storage blocked falls back to the defaults rather than failing to render.
 */

import { create } from 'zustand'
import { createJSONStorage, persist } from 'zustand/middleware'

/** brief §29's two modes. Sail Mode is the default; its value is what it omits. */
export type UiMode = 'sail' | 'debug'

/**
 * brief §30's visual list, one key per toggle, in the order the brief writes
 * them. `OVERLAY_KEYS.length` is the 16 of task 8.3.
 */
export const OVERLAY_KEYS = [
  'trueWind',
  'apparentWind',
  'velocity',
  'acceleration',
  'sailForce',
  'boardForce',
  'rudderForce',
  'hullForce',
  'totalForce',
  'yawMoment',
  'heelingMoment',
  'rightingMoment',
  'sailCe',
  'foilCentres',
  'boomRate',
  'sheetTension',
] as const

export type OverlayKey = (typeof OVERLAY_KEYS)[number]

/** The series the time-series charts can show. */
export const CHART_KEYS = [
  'heelDeg',
  'sheetTension',
  'boatSpeed',
  'alphaSail',
  'yawRate',
  'rollRate',
  'apparentWindSpeed',
  'rightingMoment',
] as const

export type ChartKey = (typeof CHART_KEYS)[number]

/** The five the PRD calls the sensible defaults. */
const DEFAULT_CHARTS: readonly ChartKey[] = [
  'heelDeg',
  'sheetTension',
  'boatSpeed',
  'alphaSail',
  'yawRate',
]

function allOff<K extends string>(keys: readonly K[]): Record<K, boolean> {
  return Object.fromEntries(keys.map((k) => [k, false])) as Record<K, boolean>
}

/** Chart sampling rate, Hz. Not the frame rate and not the physics rate. */
export const DEFAULT_SAMPLE_HZ = 20
export const SAMPLE_HZ_CHOICES = [5, 10, 20, 50] as const

/** How many samples each chart keeps. At 20 Hz this is 30 s of history. */
export const CHART_CAPACITY = 600

export interface UiState {
  mode: UiMode
  /**
   * The mode was chosen by a person, rather than being the default.
   *
   * v2 section 09 asks for two things at once: a **new** user starts in Sail
   * Mode, and a **deliberately selected** Debug Mode survives. Both already
   * follow from `mode` alone — but only by inference, and the inference is
   * exactly the kind that a later change quietly invalidates: the moment
   * anything else sets `mode` (a scenario that suggests a mode, a deep link, a
   * tutorial step), a stored `'debug'` stops meaning "they asked for it" and
   * the guarantee becomes untestable.
   *
   * So the distinction is *recorded* rather than inferred. It is set by
   * {@link UiState.setMode} and {@link UiState.toggleMode} and by nothing else
   * — in particular not by the layout, which may **arrange** the modes but may
   * never **choose** one for the player — and `ui/Layout.tsx` publishes it as
   * `data-mode-chosen` so a browser test can tell the two apart from outside.
   */
  modeChosen: boolean
  overlays: Record<OverlayKey, boolean>
  charts: Record<ChartKey, boolean>
  /** Newtons per pixel for the force vectors, when auto-scaling is off. */
  newtonsPerPixel: number
  /** Let the largest active vector set the scale each frame (task 8.3). */
  autoScale: boolean
  sampleHz: number
  parametersOpen: boolean
  /**
   * A parameter edit has invalidated simulation continuity and the run should
   * be reset (F8.2, brief §31). Set by the panel, cleared by the reset.
   */
  resetRequired: boolean

  setMode: (mode: UiMode) => void
  toggleMode: () => void
  setOverlay: (key: OverlayKey, on: boolean) => void
  setAllOverlays: (on: boolean) => void
  setChart: (key: ChartKey, on: boolean) => void
  setNewtonsPerPixel: (value: number) => void
  setAutoScale: (on: boolean) => void
  setSampleHz: (hz: number) => void
  setParametersOpen: (open: boolean) => void
  setResetRequired: (required: boolean) => void
}

const INITIAL = {
  mode: 'sail' as UiMode,
  modeChosen: false,
  overlays: {
    ...allOff(OVERLAY_KEYS),
    // Enough on by default that Debug Mode is useful the moment it opens,
    // without the view becoming unreadable.
    trueWind: true,
    apparentWind: true,
    sailForce: true,
    totalForce: true,
  },
  charts: { ...allOff(CHART_KEYS), ...Object.fromEntries(DEFAULT_CHARTS.map((k) => [k, true])) },
  newtonsPerPixel: 5,
  autoScale: true,
  sampleHz: DEFAULT_SAMPLE_HZ,
  parametersOpen: false,
  resetRequired: false,
}

export const useUiStore = create<UiState>()(
  persist(
    (set) => ({
      ...INITIAL,
      setMode: (mode) => set({ mode, modeChosen: true }),
      toggleMode: () =>
        set((s) => ({ mode: s.mode === 'sail' ? 'debug' : 'sail', modeChosen: true })),
      setOverlay: (key, on) => set((s) => ({ overlays: { ...s.overlays, [key]: on } })),
      setAllOverlays: (on) =>
        set(() => ({
          overlays: Object.fromEntries(OVERLAY_KEYS.map((k) => [k, on])) as Record<
            OverlayKey,
            boolean
          >,
        })),
      setChart: (key, on) => set((s) => ({ charts: { ...s.charts, [key]: on } })),
      setNewtonsPerPixel: (value) =>
        set({ newtonsPerPixel: Number.isFinite(value) && value > 0 ? value : 1 }),
      setAutoScale: (autoScale) => set({ autoScale }),
      setSampleHz: (hz) => set({ sampleHz: Number.isFinite(hz) && hz > 0 ? hz : DEFAULT_SAMPLE_HZ }),
      setParametersOpen: (parametersOpen) => set({ parametersOpen }),
      setResetRequired: (resetRequired) => set({ resetRequired }),
    }),
    {
      name: 'sailgym-ui',
      storage: createJSONStorage(() => localStorage),
      version: 2,
      /**
       * Schema 1 had no `modeChosen`.
       *
       * A stored schema-1 state exists only because someone used the app, and
       * the only way it can hold `'debug'` is if they pressed `M` or the
       * button — so the flag is recoverable exactly, and nobody is demoted out
       * of a mode they picked.
       */
      migrate: (persisted, from) => {
        const saved = (persisted ?? {}) as Partial<UiState>
        if (from < 2) {
          return { ...saved, modeChosen: saved.mode === 'debug' }
        }
        return saved
      },
      // `resetRequired` describes the *current run*, so it must not come back
      // from a previous session and demand a reset that has already happened.
      partialize: ({
        mode,
        modeChosen,
        overlays,
        charts,
        newtonsPerPixel,
        autoScale,
        sampleHz,
        parametersOpen,
      }) => ({
        mode,
        modeChosen,
        overlays,
        charts,
        newtonsPerPixel,
        autoScale,
        sampleHz,
        parametersOpen,
      }),
      // A stored file from an older build may be missing a key that has since
      // been added; the defaults fill the gaps rather than leaving `undefined`
      // where a boolean is expected.
      merge: (persisted, current) => {
        const saved = (persisted ?? {}) as Partial<UiState>
        return {
          ...current,
          ...saved,
          overlays: { ...INITIAL.overlays, ...(saved.overlays ?? {}) },
          charts: { ...INITIAL.charts, ...(saved.charts ?? {}) },
        }
      },
    },
  ),
)
