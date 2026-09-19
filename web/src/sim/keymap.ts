/**
 * Key → action mapping (brief §28).
 *
 * The mapping lives in one table so re-binding is a data change, not a code
 * change. Keys are `KeyboardEvent.key` values, lower-cased for letters; both
 * the letter and the arrow are bound, exactly as brief §28 recommends.
 */

export type Action =
  | 'steerPort'
  | 'steerStarboard'
  | 'sheetRelease'
  | 'reset'
  | 'pause'
  | 'singleStep'

export const KEYMAP: Record<string, Action> = {
  a: 'steerPort',
  arrowleft: 'steerPort',
  d: 'steerStarboard',
  arrowright: 'steerStarboard',
  ' ': 'sheetRelease',
  r: 'reset',
  p: 'pause',
  '.': 'singleStep',
}

/** Normalise a `KeyboardEvent.key` to its `KEYMAP` form. */
export function normaliseKey(key: string): string {
  return key.toLowerCase()
}

/** The action a key triggers, or `undefined` if it is unbound. */
export function actionFor(key: string): Action | undefined {
  return KEYMAP[normaliseKey(key)]
}

/** Actions that are edge-triggered (fired once on key-down), not held. */
export const EDGE_ACTIONS: ReadonlySet<Action> = new Set<Action>([
  'reset',
  'pause',
  'singleStep',
])
