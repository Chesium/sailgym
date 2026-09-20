# sailgym v2 — PRD Index and Agent Dispatch Protocol

v1 is complete and frozen: sections `01`–`10` (`M0`–`M9`) shipped a runnable
browser sailing simulator with a green eight-step gate. v2 is everything after
that, and it runs under the **same regime**: discussion notes are converted
into executable section PRDs, one agent per section, exclusive file ownership,
mechanical acceptance criteria, a handoff note per section.

## Documents

| File | Role |
|---|---|
| `../v1/brief.md` | The original specification brief. Still authoritative on **v1 scope**. Anything v2 adds beyond it is a scope extension and needs human sign-off (see below). |
| `../v1/00-foundations.md` | **Still normative.** Conventions, frames, sign conventions, equations, parameters, the WASM surface, the gate. No v2 agent may redefine anything in it. |
| `brief.md` | **Proposed, unsigned.** The v2 scope extension, answering V-A. Authoritative on nothing until its signature block is filled in; until then `../v1/brief.md` §44 stands in full. |
| `00-foundations.md` | **Written** (F12′, F14–F17), answering V-C. Records v2's *deltas* to the above — nothing more. A delta not yet approved is marked `Open` there **and** in the PRD that needs it, under `Normative deltas`. |
| `conformance.md`, `throughput.md` | **Generated**, in the style of `../v1/convergence.md` and `../v1/performance.md` — by a command in the repository, never typed. Written by sections 02 and 06. |
| `discussions/*.md` | Design notes and suggestions. **Not normative, not executable.** Source material for PRDs. |
| `prds/NN-<name>.md` | Executable section PRDs, one per milestone. Numbering restarts at `01` for v2. |
| `progress/NN-handoff.md` | Written by each section agent on completion. Read by the next. |

Precedence, unchanged from v1: **the brief outranks everything on scope**;
`v1/00-foundations.md` outranks the section PRDs on conventions; a section PRD
outranks a discussion note. If a PRD appears to contradict foundations, that is
a defect — stop and escalate rather than choosing.

## Section order

| # | Section | State | Source discussion |
|---|---|---|---|
| 01 | [Low-poly boat: 3-D geometry projected into the SVG](prds/01-boat-3d-svg.md) | **shipped** | `discussions/boat-rendering-suggestions.md` |
| 02 | [The conformance bundle: generator, Rust runner, throughput](prds/02-conformance-bundle.md) | **PRD written**, dispatchable | `discussions/cross-stack.md` §§0, 2, 3, 4.1 |
| 03 | [Python, and `wind.sample` in JAX](prds/03-jax-wind.md) | **PRD written**, dispatchable | `discussions/cross-stack.md` §§1.1, 4.1–4.3, 5 |
| 04 | [`sailgym-course`: routes, marks, guidance, passage](prds/04-course.md) | **PRD written**, blocked on V-A | `discussions/unified-agent-interface.md` §5 |
| 05 | [`sailgym-agent`: sensors, actions, cadence, helm](prds/05-agent.md) | **PRD written**, blocked on V-A | `discussions/unified-agent-interface.md` §§2–4, `discussions/ablation-spaces.md` |
| 06 | [`sailgym-env`: episode runner, autoreset, `VecEnv`](prds/06-env.md) | **PRD written**, blocked on V-A | `discussions/cross-stack.md` §§1.4, 6; `…/unified-agent-interface.md` §9 |
| 07 | [`sailgym-py`: the Gymnasium binding](prds/07-py.md) | **PRD written**, blocked on V-A | `discussions/cross-stack.md` §1 |
| — | Warp port, branch-free JAX port, invariants on both | discussion only | `discussions/cross-stack.md` §8 points 6–8 |
| — | Autopilot baselines (rule sailor, polar racer, planner) | discussion only | `discussions/autopilot-suggestions.md` |
| — | Mobile touch controls (sheet + rudder tracks, tilt later) | discussion only | `discussions/mobile-controls.md` |

Section 01 was deliberately first and deliberately small: **web-only**, no Rust,
so it could not destabilise the physics core while the v2 regime itself was being
shaken down.

**Sections 02 and 03 are dispatchable now.** They build the conformance bundle
and the first port against it, and they need only `sailgym-physics`. The reading
that unblocks them is stated in each PRD so it can be overruled: brief §44's
deferred list does not contain the vectorised alternate backend, brief §45 names
it directly, and V-A below names the four notes proposing *agents, courses, RL
and swarm* — not verification infrastructure. `brief.md` S1 and S2 put the same
question to the human explicitly.

**Sections 04–07 are blocked on V-A** and may not be dispatched until
`brief.md` is signed. They are written anyway, so that the ruling is made against
a concrete proposal rather than against an idea — and so that the cost, which
`brief.md` §4 states plainly, is visible before the ruling rather than after.

`discussions/mobile-controls.md` raises a **separate** scope question, tiered in
its own §0, and is gated on V-D below rather than on V-A.

## Converting a discussion into a PRD

A discussion note becomes a section PRD when it has all of:

1. **A scope ruling.** Either the brief already covers it, or the human has
   signed off on the extension in writing, in this repository.
2. **Tasks with exclusive `Owns:` lists** (F13.2) and `P-group:` letters
   (F13.3). Every file written by the section appears in exactly one `Owns:`.
3. **Acceptance criteria that are commands or numeric assertions** (F13.4).
   "Looks right" is never sufficient. For rendering work — which is where
   "looks right" is most tempting — the criteria are geometric identities and
   sampled references, not screenshots.
4. **A normative-deltas section**, listing every change to
   `v1/00-foundations.md` the section requires, each one marked as either
   *approved by the human, with the date* or *open, blocking*.
5. **A risk register** using `RV<n>` identifiers, so v2 risks never collide
   with v1's `R1`–`R7`.

## Dispatching a section agent

One agent per section, in order. Suggested prompt:

> You are the section agent for `docs/v2/prds/NN-<name>.md`.
>
> 1. Read `docs/v1/00-foundations.md` in full, then `docs/v2/README.md`, then
>    your section PRD in full, then the previous v2 handoff note if one exists.
>    Do not skim; the conventions in foundations are normative and you may not
>    redefine them. The discussion note your PRD came from is background, not
>    instructions — where the two differ, the PRD wins.
> 2. Execute tasks in `P-group` order (alphabetical). Tasks marked
>    **`P-group: S` you must implement yourself** — they are the contract and
>    integration tasks. Tasks sharing a letter may be delegated to subagents in
>    parallel; each subagent gets the task's `Owns:` list as its exclusive write
>    scope, plus instructions to read foundations and the section PRD first.
> 3. After each group, run `scripts/check.sh` (or `pwsh scripts/check.ps1`)
>    before starting the next.
> 4. Verify every acceptance criterion literally. They are commands and numeric
>    assertions, not descriptions.
> 5. On completion, write `docs/v2/progress/NN-handoff.md` per F13.6 and report
>    which section acceptance criteria passed, which did not, and why.
>
> Never tune a physical coefficient to make a scenario look better (brief §43).
> A **visual** constant is not a physical coefficient, but it is governed by the
> same discipline: every one carries a provenance note, and none of them may
> ever reach the physics core.

## The gate

Unchanged from v1 except for one addition, approved by the human on
**2026-09-20** and implemented by section 01, task 1.8: **`pnpm --dir web
test:unit` becomes step 8**, and the Playwright E2E run moves from step 8 to
step 9. This closes the hole recorded as an open item in
`../v1/progress/02-handoff.md` — 97 vitest tests that every section's
acceptance criteria cite but that `scripts/check` never ran.

```
1. cargo fmt --check
2. cargo clippy --all-targets -- -D warnings
3. cargo test -p sailgym-physics
4. cargo test -p sailgym-physics --test invariants --test no_shortcuts \
                                 --test convergence --test symmetry --test provenance
5. cargo test -p sailgym-physics --test regression
6. wasm-pack build crates/sailgym-wasm --target web --out-dir ../../web/src/wasm
7. pnpm --dir web typecheck
8. pnpm --dir web test:unit          ← added by v2 section 01
9. pnpm --dir web test:e2e
```

This amends `v1/00-foundations.md` F12, whose chain had 8 steps. It is recorded
here, in section 01's `Normative deltas`, and — by task 1.8 — in F12 itself,
with the date and the approval. **No other F12 change is authorised.**

Every section must leave the app runnable and the whole chain green. No
exceptions and no "will fix next section".

## Open items requiring human sign-off

| # | Item | Blocking |
|---|---|---|
| **V-A** | **Scope. A proposal now exists: [`brief.md`](brief.md), unsigned.** It puts six separable rulings (S1–S6) and states what stays deferred. S1 and S2 (conformance bundle, vectorised backend) are argued not to be reached by V-A at all; S3–S5 (courses, RL environment, non-interacting fleet) are; S6 (obstacles, collision-as-termination) touches §44's deferred list directly and should be ruled on separately. | sections 04–07; **not** 02–03 |
| **V-B** | **Rotation matrices outside `frames.rs`.** F2 says frame conversions live in `physics/frames.rs` "and nowhere else"; section 02's AC calls it "the only file in the repository containing a rotation matrix". `web/src/render/Camera.ts` has contained a 2-D view rotation since M1, so the established reading is that *view* transforms are exempt. Section 01 needs a 3-D view rotation and proceeds on that reading, with guard tests. Confirm the reading, or say otherwise before 01 is dispatched. | section 01, non-fatal |
| **V-C** | ~~Whether v2 gets its own `00-foundations.md` now~~ — **answered.** [`00-foundations.md`](00-foundations.md) exists and carries F12′ and F14–F17. Each delta is still *also* recorded in the PRD that needs it, marked `Open` until approved with a date. | none |
| **V-D** | **Mobile/touch scope, in four independent rulings.** brief §38 puts mobile "out of scope unless trivial"; brief §44 separately defers *mobile controls*, *hand-force simulation*, *detailed block-and-tackle mechanics* and *hiking/body movement*; brief §45 does **not** re-list mobile as intended direction. `discussions/mobile-controls.md` §0 splits the work into T0 (touch plumbing — plausibly §38-trivial), T1 (the hand model — web-only, but §44-deferred), T2 (tension-limited haul rate — a physics change needing an F4.3 delta) and T3 (tilt/hiking — needs a crew-position DOF, forbidden by brief §4). Each tier needs its own ruling; T0+T1 are separable from T2+T3 and touch no Rust. Adding a Playwright touch project also changes the browser targets fixed by brief §38 (see §8 of that note). | a mobile section |
| **V-E** | **The gate grows to eleven steps.** Section 02 adds `--test conformance` to step 4; section 03 adds step 10 (`ruff`) and step 11 (`scripts/py-test.sh`). Sections 04–06 extend step 3's crate list without adding a step. Each is an F12 amendment in the form section 01's D1 took, and each needs a date and an approval. Text is in [`00-foundations.md`](00-foundations.md) F12′. | sections 02, 03 |
| **V-F** | **`brief.md` S6 — obstacles.** Called out separately because it is the one proposed row that lands squarely on brief §44's deferred list (*obstacle avoidance*, *collisions*). Tasks 4.5 and 4.6 are gated on it alone and are explicitly cuttable; nothing in cross-stack §8 points 1–5 needs it. | section 04, partially |
