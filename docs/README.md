# sailgym — PRD Index and Agent Dispatch Protocol

## Documents

| File | Role |
|---|---|
| `brief.md` | The original specification brief. Authoritative on **scope**. |
| `00-foundations.md` | **Normative.** Conventions, equations, parameters, WASM surface, risks, working agreement. No tasks. |
| `01-skeleton.md` … `10-hardening.md` | Executable section PRDs, one per milestone. |
| `progress/NN-handoff.md` | Written by each section agent on completion. Read by the next. |

`00-foundations.md` outranks the section PRDs on conventions. The brief outranks
everything on scope. If a section PRD appears to contradict either, that is a
defect — stop and escalate rather than choosing.

## Section order

Sections are strictly sequential. Each one leaves a runnable browser application
and a green `scripts/check`.

```
01  Repo, toolchain, WASM, Playwright skeleton           M0
02  State, integrator, clock, controls, SVG, determinism M1
03  Wind field + deck.gl visualization                   M2
04  Hull, centreboard, rudder hydrodynamics              M3   ← deletes the M1 scaffold
05  Apparent wind, sail aero, dynamic boom               M4   ← the boat first sails
06  Physical mainsheet                                   M5
07  Roll, righting moment, capsize                       M6   ← brief §46 demo works
08  Debug instrumentation + parameter panel              M7
09  Scenarios, recording, replay                         M8
10  Invariants, convergence, performance, demos          M9
```

## Dispatching a section agent

One agent per section, in order. Suggested prompt:

> You are the section agent for `docs/NN-<name>.md`.
>
> 1. Read `docs/00-foundations.md` in full, then your section PRD in full, then
>    `docs/progress/<NN-1>-handoff.md`. Do not skim; the conventions in
>    foundations are normative and you may not redefine them.
> 2. Execute tasks in `P-group` order (alphabetical). Tasks marked **`P-group: S`
>    you must implement yourself** — they are the contract and integration tasks.
>    Tasks sharing a letter may be delegated to subagents in parallel; each
>    subagent gets the task's `Owns:` list as its exclusive write scope, plus
>    instructions to read foundations and the section PRD first.
> 3. After each group, run `pwsh scripts/check.ps1` before starting the next.
> 4. Verify every acceptance criterion literally. They are commands and numeric
>    assertions, not descriptions.
> 5. On completion, write `docs/progress/NN-handoff.md` per F13.6 and report
>    which section acceptance criteria passed, which did not, and why.
>
> Never tune a physical coefficient to make a scenario look better (brief §43).
> If you must change one, record reason, source and assumption.

## Why sections are structured this way

- **`P-group: S` tasks exist because the physics is tightly coupled.** Types like
  `BoatState`, `BoatParameters`, `ForceBreakdown` and `Diagnostics` are touched by
  every module. A solo contract task lands them first so parallel leaf tasks can
  fill in bodies in files they exclusively own without conflicting.
- **Acceptance criteria are mechanical.** A subagent cannot verify "the sail
  behaves plausibly". It can verify that `cargo test -p sailgym-physics
  aero::sail::close_hauled_drives_forward` passes.
- **Sign conventions are pinned once, in F2.** Sign drift across independently
  written physics modules is the highest-probability defect class in this project
  (R3). The named sign tests in sections 02, 04, 05 and 07 are the guard; none of
  them may be weakened by a later section.
- **Vertical slices, not subsystem slices.** brief §48 requires each milestone to
  produce a runnable simulator, which is also what makes a per-section acceptance
  gate meaningful.

## Deliberate debts, tracked

| Debt | Created | Repaid |
|---|---|---|
| Throwaway scaffold force model (R4) | 02, task 2.3 | 04, task 4.5 — with greps proving deletion |
| Boom unrestrained by any sheet | 05 | 06 |
| Roll integrates; hydrostatic righting absent | 05 | 07 (with no change to section 05 code) |
| `reward` placeholder always zero | 09 | Out of v1 scope; brief §33 asks for the hook only |

## Open item requiring human sign-off

**R2** (`00-foundations.md` §F11). brief §4 fixes the sailor amidships with no
hiking, capping the righting moment at ≈ 406 N·m. The boat may be too tender to
sail close-hauled in realistic wind. Section 07 determines this empirically. If
it proves unsailable, the proposed remedy — a *static* `sailor_pos_b.y` scenario
parameter, default 0 — needs your approval before any agent implements it. No
agent may add it unilaterally.
