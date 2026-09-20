# sailgym v2 — scope extension brief

**Status: PROPOSED. Not signed. Not yet authoritative on anything.**

This document exists to answer open item **V-A** in `docs/v2/README.md`. It is
addressed to the human, and it is the only document in `docs/v2/` that may move
an item out of `docs/v1/brief.md` §44. Until the signature block at the end is
filled in, every section that cites it stays blocked, and `docs/v1/brief.md`
remains authoritative on scope in full.

Precedence, once signed: `v1/brief.md` still outranks everything on v1 scope.
This document extends it; it does not amend or reinterpret it. Where this
document is silent, §44 stands.

---

## 1. Why an extension is needed at all

`v1/brief.md` §44 defers, among other things: *multiple boats*, *RL training*,
*multi-agent sailing*, *collisions between boats*, *obstacle avoidance*.

`v1/brief.md` §45 then names the same items as the intended architecture
direction:

```text
├── native Rust headless simulator
├── vectorized alternate physics backend
│      └── JAX / Warp experiments
├── Gymnasium/PettingZoo-style RL environment
├── obstacles
└── multi-agent sailing
```

Both readings are correct for their purpose: §44 says "not in v1", §45 says
"do not design a dead end". v1 is complete and §45's property held — the physics
crate builds and tests on the host with plain `cargo test`, the wind is a field
sampled at a position, and `state_next = step(state, controls, environment,
parameters, dt)` is the actual shape of `integrator::step`.

So the question this document puts is narrow: **which §45 items move to in-scope
now, and which stay deferred.**

## 2. What this brief proposes to move into scope

| # | Item | §44 status | Proposed |
|---|---|---|---|
| **S1** | A **conformance bundle**: a language-neutral data artifact generated from the Rust core, against which any second implementation is tested | not listed in §44 | **In scope.** §45 names the vectorised backend; a bundle is the instrument that makes one checkable. Needs no new physics and no new DOF |
| **S2** | A **vectorised alternate backend** (JAX, later Warp), held to that bundle | not listed in §44 | **In scope**, as verification infrastructure first and throughput second |
| **S3** | **Autopilots and courses** — a route, guidance, a helm, rule-based controllers | §44 defers neither by name; §45 implies both | **In scope** |
| **S4** | A **Gymnasium-style RL environment** and its Python binding | §44: *RL training* deferred | **In scope.** Note the distinction that matters: this proposes the *environment*, not a training programme. Nothing here commits to training a policy |
| **S5** | **Multiple non-interacting boats** — ghost races, fleet replay, N independent boats in one wind field | §44: *multiple boats* deferred | **In scope, restricted.** Boats share a wind field and a clock; they do not affect one another. See §3 |
| **S6** | **Obstacles as geometry and as a termination condition** — ray-castable circles and segments, contact scored as `Terminated(Collision)` | §44: *obstacle avoidance*, *collisions* deferred | **Open — the human should rule separately.** It is the one row here that touches the deferred list directly, and nothing in points 1–5 of `discussions/cross-stack.md` §8 needs it |

## 3. What stays deferred, explicitly

Restating these is the point of the document — an extension that does not say
where it stops is not an extension, it is an erasure.

- **Contact physics.** No contact forces, ever, under this brief. S6, if
  approved, senses obstacles and terminates on contact. It does not simulate one.
- **Boat-to-boat interaction of any kind.** No collisions, no right-of-way, no
  wind shadow. S5 is N boats in one field, sampling it independently. If wind
  shadowing is ever wanted, `discussions/unified-agent-interface.md` §8 records
  the seam (a `WindField` decorator, not a new force term) — it is not proposed
  here.
- **Everything else in §44** — currents, waves, heave/pitch, 6-DOF, cloth, mast
  bend, traveller/vang/cunningham, hiking, multiple crew, block-and-tackle,
  hand force, shoreline, multiplayer, CFD, sail immersion, capsize recovery,
  quantitative certification. Unchanged.
- **New degrees of freedom.** `STATE_LEN = 13` and the F8.3 index order stay
  normative. `discussions/ablation-spaces.md` §0 is right that a tiller-force or
  sheet-force action space needs new DOFs and new physics; that is a separate
  request and this brief does not make it.
- **Tuning.** `v1/brief.md` §43 is untouched and applies in full. No physical
  coefficient is adjusted to make a scenario, a policy or a benchmark look
  better. Autopilot gains and adapter gains are **not** physical coefficients —
  that distinction is exactly why they live in a different crate.

## 4. What the extension costs, stated honestly

Four new crates (`sailgym-course`, `sailgym-agent`, `sailgym-env`,
`sailgym-py`), a Python toolchain where there is currently none, and two new
gate steps. Four additions to the normative documents, collected in
`docs/v2/00-foundations.md` as F14–F17. The physics crate itself gains one new
module (a digest) and three accessors, all additive; no equation, no parameter
and no state field changes.

The strongest argument for S1 and S2 is not throughput. `docs/v1/performance.md`
already records 3 144–3 532× real time on one core, and a rayon `VecEnv` over
independent boats extends that by the core count with no loss of bit-identity.
The argument is that **a second implementation is the strongest correctness
instrument this project can acquire**: it catches the sign errors, unit slips
and branch-boundary mistakes that F11's R3 names as the highest-probability
defect class, because two independent ports rarely make the same mistake.

## 5. What a section may still not do under this brief

1. Redefine anything in `docs/v1/00-foundations.md`. F14–F17 are **additions**,
   recorded in `docs/v2/00-foundations.md`; a contradiction with F1–F13 is a
   defect to escalate, never a choice to make.
2. Implement a physical equation outside `crates/sailgym-physics` — F8, now
   restated for Python as F17.
3. Add a dependency from `sailgym-physics` to any agent, course, env, JS- or
   Python-facing crate. The arrows point one way and F8.1's host-testability
   property is what they protect.
4. Weaken a test to obtain a green gate (F13.4).

---

## Signature

This brief takes effect only when the block below is filled in by the human, in
this repository, in a commit.

```
Ruling on S1 (conformance bundle):            ____________  date: __________
Ruling on S2 (vectorised backend):            ____________  date: __________
Ruling on S3 (autopilots and courses):        ____________  date: __________
Ruling on S4 (RL environment, not training):  ____________  date: __________
Ruling on S5 (multiple non-interacting boats):____________  date: __________
Ruling on S6 (obstacles, collision-as-term.): ____________  date: __________

Signed: ______________________________________  date: __________
```

Until then: `docs/v2/prds/02` and `03` proceed, because S1 and S2 are not in the
§44 list and `docs/v2/README.md` V-A names only the four notes that propose
agents, courses, RL and swarm. `docs/v2/prds/04`–`07` are blocked.
