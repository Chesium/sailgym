# M5 — Upwind without demonstrations: decision blocks and training starts (exploratory)

**Stage 1 complete, 2026-11-17.** PPO and SAC trained from scratch, without
demonstrations, on task U: one gate 40 m dead upwind, 180 s, 20 Hz. There
were four cells:

- neither change;
- decision blocks (**B**, each action held for 0.5 s);
- randomised training starts (**S**);
- both (**B + S**).

Each cell has five training seeds at 4M decisions. That makes 40 rows: 34 new
and 6 reused from M4U. PPO's B and B + S cells add ten extended rows at 40M
decisions. Every row is post-hoc and exploratory, and none is merged into an
M4/M4U/M4X/M4P table.

**Reading:** for both learners, the declared reading *"no cell finishes →
neither change is sufficient; run the second stage"* applies.

- No selected checkpoint in any of the 50 rows finishes a validation episode
  (0/1,000).
- No held-out episode finishes: 0/6,319, over 89 groups of selected and final
  checkpoints.
- Ten times the PPO updates (the 40M rows) produce no finish either.
- The second stage is designed in the plan but not frozen or run, by user
  decision on 2026-11-17. See [Second stage](#second-stage-designed-not-run).

**What each change did (exploratory, beyond the declared readings):**

- **B fixed SAC's critic scale, but not its control.**
  - Every SAC B and B + S run keeps Q inside the feasible band
    [−22.3, +15.7]: max |Q| is 7.5–11.3, and α ends at 0.0001–0.0006.
  - Every SAC run without B leaves the band by two orders of magnitude
    (max |Q| 2,223–6,614).
  - With Q in the band, SAC B still makes no tack at its selected checkpoints
    and reaches a mean max y of only 2.6 m.
- **S supplied finishes, but only near the gate.**
  - The S and B + S cells saw 25 training finishes, all from random near-gate
    starts. That is 0.2% of their random-start episodes: 0.4% in the S cells
    and under 0.1% in B + S. No-learning pink noise scored 2% from the same
    box.
  - None of this carried back to the frozen start 40 m away. The cells with
    random starts (S and B + S) make the least upwind progress of any cell
    (mean max y 0.3–0.6 m).
- **PPO B needs M4U's update count to match M4U's behaviour, and then does no
  better.**
  - At 4M decisions, PPO B makes about 195 updates and tacks in 0/100
    selected validation episodes.
  - At 40M (about 1,950 updates) it tacks in 18/100, with a mean max y of
    11.3 m. That is close to PPO "neither" at 4M: 20/100 and 10.1 m.
- **One frozen-start finish in training.** Extended PPO B seed 0 made M5's
  only frozen-start training finish, in training episode 2,650 (after
  4M decisions), at 169.8 s of 180. Its selected checkpoint never finishes.

## Protocol

- **Freeze:**
  - [`configs/M5_freeze.json`](../../experiments/ese3990/configs/M5_freeze.json)
    was written 2026-11-17 at 03:53:58 UTC, before the main queue started at
    03:54:12 UTC. Its SHA-256 is `82cc283a…`.
  - It records the cells, seeds 0–4, the start box, the configurations, the
    threshold, the Q band, the readings, the six reused M4U rows (with their
    hashes) and the SHA-256 of 22 sources.
  - The follow-on stages (extended training, audits, held-out evaluation) added
    new source files only. Those are sealed in
    `runs/ese3990/M5/followup_plan.json`.
- **B (decision blocks):**
  - Each action is held for 10 decisions (0.5 s).
  - The block reward is the discounted sum of its per-decision rewards, and γ
    per block is 0.99975¹⁰, so the objective is unchanged.
  - A block ends early at a finish, a failure, the time limit or the budget
    cutoff, and bootstrapping uses its actual length.
  - Every other setting keeps its value, now counted in blocks: PPO's rollout
    is 2,048 blocks, SAC's replay holds 400k blocks, and SAC updates once per
    block.
- **S (training starts):**
  - Training episodes alternate between the frozen start and a random start:
    x ∈ [−10, 10] m, y ∈ [25, 39] m, uniform heading, at rest.
  - Wind, route, gate, reward and time limit are unchanged.
  - Calibration and every evaluation use the frozen start.
- **Budget:**
  - Each row gets 4M actual 20 Hz decisions, including the common 10,000
    calibration decisions at the frozen start.
  - At equal decisions, the B cells make ten times fewer updates. PPO's 40M
    rows exist for that reason, and they are never compared with a 4M row as
    if the two were equal.
  - PPO and SAC are matched by decisions and seeds, not by CPU time.
- **Evaluation:**
  - Every row uses the frozen U contract: frozen start, 20 Hz, γ = 0.99975.
  - Validation uses M4U's 20-episode validation set at M4U's checkpoints, with
    M4U's selection rule: most validation finishes, then highest mean return,
    then earliest checkpoint.
  - B and B + S policies are queried every 10 decisions and their action is
    held (`m5.HeldAction(policy, 10)`).
- **Held-out:**
  - Only validation-selected and final checkpoints are evaluated. Duplicates
    are merged, leaving 89 groups.
  - They were frozen (`heldout/freeze.json`, SHA-256 `1ef2d5d7…`) before any
    test or shift episode ran.
  - Each group gets the nominal condition, 50 test conditions and 20 shift
    conditions: 71 episodes.
- **Threshold, declared before results:** a cell *finishes* if at least 3 of
  its 5 seeds have a selected checkpoint with at least 1 finish in 20
  validation episodes.

### Preparation

| Training-only random-start box (no learning, pink noise, 0.5 s hold, σ = 0.3) | Finishes | Exact replays | Gate (≥ 1%) |
|---|---:|---:|---|
| Draft: x ∈ [−25, 25] m, y ∈ [0, 35] m | 0/100 | 100/100 | Fails |
| Amended: x ∈ [−10, 10] m, y ∈ [25, 39] m | 2/100 | 100/100 | Passes |

The draft box was amended before the freeze, as the plan allowed. Evidence is
in [`M5_start_box.json`](M5_start_box.json).

Before the freeze, eight 100k-decision pilots (both learners × four cells,
seed 100) checked plumbing and timing only. All 572 of their episodes replay
exactly ([`M5_smoke_audit.json`](M5_smoke_audit.json)). Their control outcomes
are excluded from every reading.

## Results: main rows, 4M decisions

Validation columns are for the selected checkpoints, 5 seeds × 20 episodes.
Held-out columns are for the selected checkpoints, 5 seeds × 71 episodes, of
which 250 are test episodes. *Untrained selected* counts seeds whose selected
checkpoint is the one at 10,000 decisions, after calibration only. *Training
finishes* counts finished training episodes, with the number of random-start
training episodes in brackets.

| Learner | Cell | Seeds meeting threshold | Val finishes | Val episodes with a tack | Mean max y (m) | Held-out finishes | Test episodes with a tack | Untrained selected | Training finishes |
|---|---|---|---|---|---:|---|---|---:|---|
| PPO | neither | 0/5 | 0/100 | 20/100 | 10.1 | 0/355 | 40/250 | 0 | 0 |
| PPO | B | 0/5 | 0/100 | 0/100 | 4.8 | 0/355 | 0/250 | 1 | 0 |
| PPO | S | 0/5 | 0/100 | 0/100 | 0.6 | 0/355 | 0/250 | 0 | 11 (of 2,816) |
| PPO | B + S | 0/5 | 0/100 | 2/100 | 0.4 | 0/355 | 10/250 | 2 | 1 (of 2,788) |
| SAC | neither | 0/5 | 0/100 | 1/100 | 3.7 | 0/355 | 0/250 | 1 | 0 |
| SAC | B | 0/5 | 0/100 | 0/100 | 2.6 | 0/355 | 0/250 | 0 | 0 |
| SAC | S | 0/5 | 0/100 | 0/100 | 0.3 | 0/355 | 0/250 | 2 | 11 (of 2,847) |
| SAC | B + S | 0/5 | 0/100 | 0/100 | 0.3 | 0/355 | 0/250 | 1 | 2 (of 2,798) |

SAC's critic, over each whole run:

| SAC cell | Seeds with Q inside [−22.3, +15.7] | max \|Q\| | Final α |
|---|---|---|---|
| neither | 0/5 | 2,223–6,614 | 0.018–0.117 |
| B | 5/5 | 7.5–9.4 | 0.00009–0.00034 |
| S | 0/5 | 3,649–5,717 | 0.0002–0.0081 |
| B + S | 5/5 | 8.7–11.3 | 0.0002–0.0006 |

- **Tacking is rare and confined to a few seeds.**
  - PPO "neither" seed 2 tacks in 17/20 validation and 30/50 test episodes.
    Seed 1 tacks in 3 and 10.
  - PPO B + S seed 4 tacks in 2 and 10. SAC "neither" seed 2 tacks in 1
    validation episode.
  - Every other main row has no tack at its selected checkpoint.
  - The PPO "neither" seeds 0–2 are the reused M4U rows. Their 40 test tacks
    are M4U's.
- **Upwind progress without tacking.** The largest mean max y is PPO
  "neither" seed 3: 27.9 m on validation and 24.9 m on test, with no tack. It
  sails one long board to the side, ending at x = 39–49 m, and never comes
  within 33 m of the gate.
- **Seven of 40 rows select the untrained checkpoint.** At the selected
  checkpoints, 999 of 1,000 validation episodes time out, and one fails. Mean
  selected validation return per cell lies between −5.17 and −5.34. The
  selection rule therefore ranks near-identical timeouts.
- **Final checkpoints of the S cells fail more often:**
  - PPO S: 58/355 held-out episodes end out of bounds, capsized or with a
    missed mark.
  - SAC S: 71/355.
  - Selected checkpoints, per cell: at most 6/250 test and 11/100 shift.
- **Rudder travel** is decision-sampled, in rad per episode, at selected
  checkpoints:
  - B or B + S: up to 52 rad for SAC and 24 rad for PPO at 4M. The extended
    PPO rows reach up to 49 rad (B) and 61 rad (B + S).
  - Every other cell: at most 12 rad.
  - There is no finish, so no sculling finish exists to flag. No travel
    threshold was declared.
- **Cost.** Wall times per run, on CPU with six concurrent workers:

  | Cell | PPO | SAC |
  |---|---|---|
  | B, B + S | 4 min | 16–18 min |
  | S | 8 min | 94–136 min |
  | neither, new seeds | 8 min | 119 min |
  | neither, reused M4U rows | 18 min | 194 min |

  The reused rows ran under different concurrency, so their times are not
  comparable.

## Extended PPO rows, 40M decisions

These rows have about 1,950 PPO updates, M4U's count. They are labeled as an
extended budget and are not compared with the 4M rows as if equal.

| Cell | Seeds meeting threshold | Val finishes | Val episodes with a tack | Mean max y (m) | Held-out finishes | Test / shift episodes with a tack | Training finishes |
|---|---|---|---|---:|---|---|---|
| B | 0/5 | 0/100 | 18/100 | 11.3 | 0/355 | 27/250 / 17/100 | 1 (frozen start, seed 0) |
| B + S | 0/5 | 0/100 | 0/100 | 0.2 | 0/355 | 0/250 / 0/100 | 1 (random start, seed 4) |

- **Continuations, exactly.** At every shared checkpoint up to 3M, each
  extended run's validation results are identical to its 4M row: 10/10 runs,
  5 shared checkpoints each. B + S seed 4's training finish is the same
  episode as in its 4M row.
- In the B cell, tacking appears at 40M in seed 2 (16/20 validation, 26/50
  test) and seed 4 (2/20 validation). Seed 4 reaches a mean max y of 23.5 m on
  test, but none finishes.
- B + S at 40M makes almost no upwind progress, and its rudder travel is the
  largest of any cell (up to 61 rad).

## Which declared reading applies

| Observation (declared) | Applies? | Evidence |
|---|---|---|
| B finishes, S does not → the decision interval was the bottleneck | **No** | 0/5 seeds in every cell, both learners |
| S finishes, B does not → discovering a first finish was the bottleneck | **No** | as above |
| Only B + S finishes → both are needed, as in Bink's setup | **No** | as above |
| No cell finishes → neither change is sufficient; run the second stage | **Yes, PPO and SAC** | 0/1,000 selected validation episodes and 0/6,319 held-out episodes finish |
| A PPO cell finishes only at 40M → PPO's B result is update-bound | **No** | Both 40M cells have 0/5 seeds meeting the threshold |
| One learner finishes where the other does not | **No** | Neither learner finishes anywhere |

## What this means (inference)

- **Neither change is enough at these budgets, alone or together.**
  - B removes the critic-scale failure M4X found in SAC: Q stays in the band
    and α collapses. SAC B is still no closer to a tack. Critic scale was
    therefore not SAC's only obstacle on U.
  - The S runs did see finishes from near the gate, but at a lower rate than
    no-learning pink noise from the same box. Credit for those finishes did
    not reach the frozen start, 25–39 m further away, within 4M decisions.
- **PPO's B cell behaves like the M4U configuration once its update count
  matches,** at 40M decisions. It makes some upwind progress and tacks in a
  few seeds, but never finishes. The extra decisions buy back the lost
  updates and nothing more.
- **Tacks are not the missing piece on their own.** The rows that tack most
  (PPO "neither" seed 2, extended PPO B seed 2) reach 17–22 m upwind and stop
  there. The row with the most upwind progress never tacks. This matches M4P,
  where random noise tacked without beating.

## Second stage: designed, not run

The "no cell finishes" reading calls for the second stage. By user decision on
2026-11-17, the stage is designed but not frozen or run. The draft, including
the post-hoc best-cell rule it needs (the plan never defined one, and every
cell has zero finishes), is in the plan under
[M5 second stage (M5b)](ESE3990_SailGym_Project_Plan.md#m5-second-stage-m5b--draft-for-review-not-frozen).
It proposes one change per arm on each learner's "neither" cell: SAC with no
entropy term in the critic target, then pink noise at σ = 0.3; PPO with 8
environments per update, then initial σ = 0.3. Three choices need review
before the stage's own freeze: the best-cell rule, the form of SAC's pink
noise, and the shape of PPO's 8-environment batch.

## Artifacts and reproduction

- [`M5_results.json`](M5_results.json) holds every number in this report:
  per-row and per-cell validation, held-out, Q, α, rudder travel, training
  finishes and readings. It is generated without running any episode from
  the audited artifacts under `runs/ese3990/M5`, whose SHA-256 values it
  records.

  ```sh
  env -u PYTHONPATH uv run --frozen --no-sync python experiments/ese3990/m5_report.py
  ```
- **Exact replays: 169,586 episodes, all exact.**
  - Main rows, including the six reused M4U rows: 49,982 stored episodes
    (calibration, training, probe and validation).
  - Extended rows: 113,285.
  - Held-out: 6,319.

  The evidence is in `runs/ese3990/M5/{main_audit,extended_audit}.json` and
  `heldout/{freeze,complete,replay_audit}.json`.
- The claim index is [`M5_claims.json`](M5_claims.json). Commands are in the
  [experiment README](../../experiments/ese3990/README.md). The execution
  history is in the [M5 handoff](../handoffs/ese3990-m5.md).

## Deviations and limitations

- **Start box amended before the freeze.** The draft box failed its declared
  check (0/100), so it was moved nearer the gate (2/100). Both checks are
  retained.
- **Short final PPO rollout.** A final rollout shorter than the 256-decision
  minibatch uses its actual size as the minibatch. This is declared in the
  freeze.
- **Reused M4U rows logged only max |Q| and mean Q.** For those three SAC
  rows, the band classification uses max |Q|. All three are far outside the
  band (2,223–5,040).
- **Training-finish counts** come from the stored training episodes. Every
  one of those episodes replays exactly in the main and extended audits.
- **Measurement limits.**
  - There are 20 validation episodes per checkpoint and five seeds per cell,
    with no confidence intervals.
  - Tack counts use the frozen M4U diagnostic thresholds.
  - Rudder travel is sampled at decisions, with no declared threshold.
  - Held-out evaluation covers only selected and final checkpoints.
- **Wall times** come from six concurrent CPU workers. They are not isolated
  measurements of algorithm speed.
