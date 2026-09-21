/**
 * The single entry point through which the browser reaches the Rust core.
 *
 * Nothing in `web/` implements physics (F8, brief §23): this module only
 * instantiates the `wasm-pack --target web` bundle produced by
 * `scripts/build-wasm.*` into `web/src/wasm/` (a generated, gitignored
 * directory) and hands back the constructors declared by F8.2.
 */
import init, { Sim } from '../wasm/sailgym_wasm.js'

/**
 * The `Sim` surface, taken straight from the `wasm-pack`-generated
 * declarations rather than restated here: a hand-written mirror is one more
 * place for the TypeScript side to drift from the Rust one (F8).
 *
 * This is why nothing in this file changed when v2 section 10 added
 * `recording_capacity`, `recording_bytes_per_frame`, `recording_full`,
 * `episode_identity_json` and `episode_comparability_json` to `Sim`: the
 * declarations regenerate at gate step 6 and the new methods are typed the
 * moment they exist. The same finding was recorded for the six geometry fields
 * of v2 section 01 and the six actuator limits of v2 section 09
 * (`docs/v2/progress/09-handoff.md` §2.2).
 *
 * v2 section 11 is the fourth time, and it is now a rule rather than a
 * coincidence: `practice_tasks_json`, `start_practice`, `retry_practice`,
 * `cancel_practice` and `practice_state_json` are typed here without a line
 * being written, because `SimHandle` **is** the generated declaration.
 * Restating the surface by hand is the one thing that would break it.
 */
export type SimHandle = Sim

export interface WasmModule {
  Sim: typeof Sim
}

/**
 * The in-flight (or settled) initialisation. Caching the *promise* rather than
 * the resolved value means concurrent callers share one `init()` and every
 * caller observes the identical `WasmModule` object.
 */
let pending: Promise<WasmModule> | null = null

/** The one module object every successful `loadWasm()` call resolves to. */
const moduleExports: WasmModule = { Sim }

/**
 * Idempotent: repeated calls return the same initialised module object
 * (`===`), and concurrent calls share a single `init()`.
 *
 * A failed initialisation clears the cache so a later call may retry; the
 * identity guarantee only ever concerns the success path.
 */
export async function loadWasm(): Promise<WasmModule> {
  if (pending === null) {
    const started = init().then(() => moduleExports)
    // Do not poison the cache forever on a transient failure. The caller still
    // receives `started` and owns its rejection, so this handler adds no
    // unhandled rejection of its own.
    started.catch(() => {
      if (pending === started) {
        pending = null
      }
    })
    pending = started
  }
  return pending
}
