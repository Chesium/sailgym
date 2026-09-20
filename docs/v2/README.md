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
| `00-foundations.md` | **Pending.** Will record v2's *deltas* to the above — nothing more. Until it exists, every delta is recorded in the PRD that needs it, under a heading `Normative deltas`, and escalated to the human there. |
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
| 01 | [Low-poly boat: 3-D geometry projected into the SVG](prds/01-boat-3d-svg.md) | **PRD written** | `discussions/boat-rendering-suggestions.md` |
| — | Unified agent interface | discussion only | `discussions/unified-agent-interface.md` |
| — | Ablating action and observation spaces | discussion only | `discussions/ablation-spaces.md` |
| — | Autopilot baselines (rule sailor, polar racer, planner) | discussion only | `discussions/autopilot-suggestions.md` |
| — | Gymnasium + a JAX/Warp second stack | discussion only | `discussions/cross-stack.md` |

Section 01 is deliberately first and deliberately small: it is **web-only**, it
touches no Rust, and it therefore cannot destabilise the physics core while the
v2 regime itself is being shaken down. The four remaining notes all describe
post-v1 *scope extensions* (brief §44 defers autopilots, multiple boats, RL,
obstacles); none of them may be converted into a PRD until the scope question
in `discussions/unified-agent-interface.md` §0 has a human answer.

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
| **V-A** | **Scope.** brief §44 defers multiple boats, RL, multi-agent sailing, collisions and obstacle avoidance; brief §45 lists the same items as the intended direction. Four of the five discussion notes need a v2 brief (or a brief §44 amendment) before they can become PRDs. | sections after 01 |
| **V-B** | **Rotation matrices outside `frames.rs`.** F2 says frame conversions live in `physics/frames.rs` "and nowhere else"; section 02's AC calls it "the only file in the repository containing a rotation matrix". `web/src/render/Camera.ts` has contained a 2-D view rotation since M1, so the established reading is that *view* transforms are exempt. Section 01 needs a 3-D view rotation and proceeds on that reading, with guard tests. Confirm the reading, or say otherwise before 01 is dispatched. | section 01, non-fatal |
| **V-C** | Whether v2 gets its own `00-foundations.md` now, or keeps recording deltas per-PRD until there are enough to justify one. | none |
