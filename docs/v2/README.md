# sailgym v2 — milestone plan and PRD index

The next milestone is **sail, inspect, retry**: an explainable sailing playground that works on a phone and produces trustworthy comparisons. Photorealism and a full 3-D world are not goals. Section 01's low-poly geometry projected into SVG has shipped.

## Delivery order

Section IDs are stable document identities, **not dispatch order**. The JAX PRDs added in commit `6358c92` retain their IDs and detailed tasks. The four playable-milestone PRDs precede them:

| Order | PRD | Deliverable / prerequisite |
|---|---|---|
| Shipped | [01 — Boat rendering](prds/01-boat-3d-svg.md) | Existing SVG renderer; retain it |
| M-next.1 | [08 — Physics consistency](prds/08-physics-consistency.md) | Reviewed reduced-model corrections and identified baseline |
| M-next.2 | [09 — Touch and readability](prds/09-touch-readability.md) | Rate controls, release button, responsive sail view; uses 08 limits |
| M-next.3 | [10 — Trustworthy replay](prds/10-trustworthy-replay.md) | One recorded data source, schema migration and experiment identity; depends on 08 |
| M-next.4 | [11 — Guided practice](prds/11-guided-practice.md) | Three challenges, inspect/retry, two-attempt comparison; depends on 09–10 |
| Shipped | [02 — Conformance bundle](prds/02-conformance-bundle.md) | Delivered 2026-09-21 after 08–11: the bundle, its generator, the Rust runner in gate step 4, and the measured multi-core throughput ([conformance.md](conformance.md), [throughput.md](throughput.md), [progress/02-handoff.md](progress/02-handoff.md)) |
| Research follow-on.1 | [03 — JAX wind](prds/03-jax-wind.md) | Wind-only verification pilot; 02's bundle is the artifact it is held against, and F16.6's measured kernel gap is in that bundle's manifest. Not a full sailing backend |
| Later | [04 — Courses](prds/04-course.md) | Ordered course semantics; no Python/JAX dependency |
| Later | [05 — Agent interface](prds/05-agent.md) | After 04 and identity contract; external manual commands, minimal sensors |
| Later | [06 — Environment](prds/06-env.md) | After 04–05; distinct independent episodes, boundaries and decision logs |
| Later | [07 — Python binding](prds/07-py.md) | After 03 toolchain and 06; Gymnasium interface, not policy training |

M-next ships when 08–11 pass their acceptance criteria. It does **not** wait for Python, a new controller framework, live fleets or JAX. A recorded ghost and one rule sailor on a short course are the next product slice; their web integration still needs a PRD. Existing 04–07 do not deliver that UI. Full-step JAX/Warp ports and training remain separate decisions.

## Status and authority

This revision is an authorized **planning update**, not a claim that runtime work or numerical validation has happened. All unshipped sections are proposed. Section 02's deltas (D1, D2 = F16, D3 = brief S1) were resolved when it was dispatched on 2026-09-21 and are recorded in `docs/v2/progress/02-handoff.md` §1 and in F16.9; **03 is still not “dispatchable now”** while its own normative deltas remain open. [brief.md](brief.md) records scope proposals; [00-foundations.md](00-foundations.md) records explicit model/API deltas. No signature or numerical approval is inferred from drafting these notes.

The v1 brief and foundations describe the shipped baseline. A v2 implementation that changes it must record the exact approved delta, with evidence, rather than silently reinterpret v1. Section PRDs override discussion sketches, and this index's delivery order overrides the old numeric ordering. [Deferred features](discussions/deferred-features.md) tracks every v1 brief §44 item separately.

## Execution protocol

Read v1 foundations, this index, the v2 brief and foundations, the section PRD, and the **dependency handoffs**, not simply the previous-numbered handoff. Each PRD has exclusive task ownership, P-groups, acceptance, risks and a completion handoff. `S` denotes a sequential contract/integration task at its listed position; alphabetic groups run only after their prerequisites exist. Same-letter tasks may run independently only where their Owns lists do not overlap. No section starts while its required normative delta is unresolved.

Keep changes scoped to the section. Compare no-change guards against that section's starting revision, not all changes since v1. Record pre-existing failures separately. Do not weaken assertions to regenerate goldens. Run the existing full gate after each completed group as required by F13; use focused checks during edits. A handoff reports commands, measured values, migration effects, risks and unresolved limitations. Documentation drafting alone does not warrant running the application gate.

## Gates and open decisions

The **implemented** gate has nine steps: fmt, clippy, physics and task tests, the audit targets (invariants, no-shortcuts, convergence, symmetry, provenance and — since section 02 — conformance), regression, WASM build, TypeScript checking, web unit tests and browser E2E. Section 01 added the unit-test step; section 11 added `-p sailgym-task` to step 3; section 02 added `--test conformance` to step 4 on 2026-09-21 by human approval. The chain is still nine steps. See the actual `scripts/check.sh` and `scripts/check.ps1` for commands.

| Item | Proposal / dependency |
|---|---|
| V-A | Scope rows S1–S9 in the brief; approve only the implementation slice being started |
| V-B | Rendering view transforms are presentation only; section 01 shipped, retain its guard tests |
| V-C | v2 foundations exist; proposed text is not evidence of implementation |
| V-D | Basic mobile rate controls promoted to M-next; hand model, position targets and tilt remain deferred |
| V-E | **Discharged for 02**: `--test conformance` is in step 4, approved 2026-09-21, and `docs/v2/00-foundations.md` F12′ and F16.9 record it. 03 adds Python checks as steps 10–11; 04–06 extend Rust crate coverage when built. Do not install those gates before their tools exist |
| V-F | Obstacle tasks 4.5–4.6 remain excluded unless separately selected; no speculative raycasting |
| V-G | 08 resolves GZ shape, slack tension, sheet minimum and baseline migration with measured evidence |
| V-H | 10 defines recording compatibility; 11 adds task metadata without a second recorder |

`conformance.md` and `throughput.md` are **generated outputs of section 02**, each written by a named command in the repository (`gen_conformance --write` and `vec_bench --write`) and neither a physical validation report: conformance is implementation agreement, and throughput is one machine. Implementation handoffs belong in `progress/`.
