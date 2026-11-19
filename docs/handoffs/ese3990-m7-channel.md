# ESE3990 M7 · Channel beat · run record

Plan: [M7 section](../ese3990/ESE3990_SailGym_Project_Plan.md#m7--channel-beat-string-several-tacks-together--started-november-20). Post-hoc and exploratory. Budget: 4 hours of learner tuning from 01:10 on November 20, 2026 (ends about 05:10).

## Setup

- Task: U with x bounds [−10, 10] m (`m7_channel_mpc.channel`). Everything else is the frozen U contract.
- Code (new files only; M6 sources unchanged):
  - `experiments/ese3990/m7_channel.py` runs `m6_tune` train, `m6_tune_eval` eval and `m6_tune_heldout` with their `Task.load("U")` swapped for the channel. Eval replays in-process, because `u_diagnostics`' spawned workers would reload open-water U. Each store's run record carries `m7_channel_half_m`, since the episode index still says task `U`.
  - `experiments/ese3990/m7_channel_mpc.py` is the planner feasibility check; `m7_channel_plot.py` draws its tracks.
- Launch: `runs/ese3990/M7/launch.sh NAME SEED [args]` (CPU) and `launch_gpu.sh` (GPU venv at `runs/ese3990/M7/gpuenv`, rebuilt from the M6 recipe). Curves: `cd experiments/ese3990 && ../../.venv/bin/python m7_channel.py report [PREFIX]`.
- A smoke run (60k decisions) went through train, eval on the panel and exact replay of 40 episodes, then was deleted.

## Pre-training evidence

- Zero-shot: the 9 converged M6 PPO policies finish 0/450 test episodes at ±15, ±10 and ±7.5 m, all out of bounds.
- Planner H160_K32, search seed 0, validation-000 to -007 at ±10 m: 8/8 finish, mean 49.1 s, 2 tacks each. Figure: `docs/ese3990/figs/M7_channel_mpc_half10_tracks.png`.

## Log

- 01:11 · `c1`: the M6 tuning configuration unchanged (`--norm fixed --head target --repeat 10 --hidden 256 --decisions 10e6`). PPO seeds 0–9 on CPU; SAC seeds 0–4 on GPU.
- 01:27 · `c1` PPO, 10 seeds at 10M: 0/40 panel finishes on every seed, every panel episode a timeout. Mean max y is 17–30 m and log σ has fallen to about −1.2. The policy avoids the wall (−10) by hovering, and never tacks. This is the timeout trap with the wall as the threat. `c1` SAC at 4M: seeds 1 and 3 already 40/40 on the panel; seeds 0, 2 and 4 are at 0 (max y 10–13 m).
- 01:28 · `c2` PPO variants, seeds 0–2, launched together:
  - `ent`: entropy coefficient 0.01 instead of 0.001, to keep exploring;
  - `rand`: half the training starts uniform in x ∈ [−8, 8], y ∈ [0, 38], any heading;
  - `curr`: a width curriculum, ±30 m shrinking 5 m whenever the panel at the current width finishes ≥ 30/40, down to ±10 m; the logged `eval` is always at ±10 m; 20M decisions;
  - `rate`: M6's rate head with initial log σ −1.5.
- 01:42 · `c2` at 10M: `ent`, `rand` and `rate` are 0/40 on all three seeds. `rand` even hovers near the start (deterministic max y 2–3 m), and its training episodes finish 1/332, random starts near the gate included. The curriculum shows where PPO breaks. Seeds 0 and 2 learn ±30 m (40/40 by 6–7M) and shrink cleanly to ±15 m at 40/40, then collapse to 0 at ±10 m and stay there. Seed 1 learns ±30 m (39/40) and collapses at ±25 m. By geometry (close-hauled course over ground about 34° off the wind, as the planner sails), one tack still fetches the 5 m gate down to about ±12.5 m; ±10 m needs a second tack. PPO forgets the one-tack policy once the wall stops it, instead of adding a tack. `c1` SAC at 8M: seeds 1, 3 and 4 are at 40/40; seeds 0 and 2 are at 0.
- 01:44 · `c3` PPO with per-episode channel width (`MixTask`: half-width uniform in [10, H] m, logged `eval` at ±10 m), seeds 0–2, 20M decisions:
  - `mix`: H = 30, fixed;
  - `mixcurr`: H from 30, shrinking 5 m at ≥ 30/40 on the ±H panel;
  - `mix20`: H = 20, fixed.
  A policy that tacks at the ±10 m walls is valid at every width, so narrow episodes push toward early tacks while wide ones keep a finishing signal.
- 01:48 · `c1` SAC at 10M: seeds 1, 3 and 4 finish 40/40 on the panel; seeds 0 and 2 stay at 0. Seed 0 is in the hover trap, with α ≈ 1e-4, Q ≈ −5.5 (the timeout value) and 38/40 panel timeouts. That makes SAC 3/5, against M6's 4/5. `c2_ppo_curr` stays at 0 at ±10 m through 20M.
- 01:49 · SAC seeds 5–9 with the `c1` configuration, for a 10-seed estimate. `c3_sac_mix` is SAC with per-episode width in [10, 30] m, seeds 0–4. Held-out evaluation of `c1_sac` s0–4 (selected and final checkpoints) is running.
- 01:54 · `c3` PPO at 7–8M: 0/40 at ±10 m and 0/40 at the training maximum (±30 or ±20 m) on every run. Mixing in narrow episodes stops PPO from learning even the wide case: fixed ±30 m learned at 5–7M in `c2_ppo_curr`. `mixcurr` was stopped, because it is identical to `mix` until a shrink and none came. SAC seed 3's panel tracks at 10M follow the planner's route: tack near x ≈ 9 m at y ≈ 6 m, again near x ≈ −8 m at y ≈ 25 m, then fetch the gate.
- 01:54 · `c4` reasoning: every one-tack policy in M6 and `c2_ppo_curr` tacks port → starboard only (it starts on port, heading east with the wind from the north). At ±10 m the second tack is starboard → port, the mirror manoeuvre, which a one-tack policy has never done. M6's exactly mirror-equivariant nets (`--equivariant`) give that manoeuvre for free. Seeds 0–2:
  - `eq`: equivariant, fixed ±10 m, 10M;
  - `eqcurr`: equivariant with a 30 → 10 m curriculum in 2.5 m steps at ≥ 30/40, 20M;
  - `curr25`: the same gentle curriculum without equivariance, 20M.
- 02:05 · Held-out evaluation of `c1` SAC in the ±10 m channel (`runs/ese3990/M7/train/heldout/c1_sac_s0-4_10M.json`). Selected checkpoints are the 10M ones on every seed. Seeds 1, 3 and 4: validation 20/20 each, test 50/50 each, shift 20, 9 and 20 of 20 (seed 3's 10 other shift episodes go out of bounds). Mean finish is 43–49 s, against the planner's 49.1 s on validation-000 to -007. Every finish has a counted tack, and all episodes replay exactly. Seeds 0 and 2 finish nothing; their episodes time out. Totals: validation 60/100, test 150/250, shift 49/100. `c4_ppo_curr25` (2.5 m steps): seed 1 is at ±22.5 m with 40/40 and seed 2 at ±25 m with 40/40 by 6M; seed 0 has just learned ±30 m.
- 02:14 · `c3` PPO mixes at 20M: 0/40 at ±10 m and 0/40 at their widest width on all six runs. `c4_ppo_eq` at 10M: 0 on all three seeds. `c4_ppo_eqcurr` was still 0 at ±30 m at 12M and was stopped; equivariance hurts here as it did in M6. **`c4_ppo_curr25` works.** Seed 1 reached ±10 m at 11M and scored 0 → 13 → 40 → 40 (40/40 at 13M and 14M). Seeds 0 and 2 reached ±10 m at 13–14M with 40/40 at ±12.5 m first. They are now 0/40, but their episodes end out of bounds rather than in timeouts, so they are still sailing; seed 1 also passed through that state. Launched seeds 3–9 of `curr25` for a 10-seed estimate. Also `c5_ppo_curr125`: steps of 1.25 m, panel every 0.5M, 25M decisions, seeds 0–4.
- 02:19 · `curr25` seeds 0 and 2 are still 0/40 at ±10 m after 3–4M there. Added `--backoff N` to the curriculum: after N consecutive 0/40 panels at a shrunk width, widen one step (never beyond the start). A stub check confirmed the 15 → 12.5 → 10 → 10 → 12.5 → … schedule. Launched `c6_ppo_back`: 2.5 m steps, back-off after 2 panels, 25M decisions, seeds 0–4.
- 02:40 · Held-out evaluation of `curr25` seeds 0–2 at the selected 20M checkpoints (`heldout/c4_ppo_curr25_s0-2_20M.json`). Seed 1: validation 20/20, test 50/50, shift 20/20. Seed 0, caught mid-recovery at the end of its budget: 17/20, 37/50 and 3/20. Seed 2: 0. Live `curr25` seeds 3–9 at 15M: seeds 6, 7 and 8 have crossed to ±10 m (25–40/40), seeds 4 and 5 are at ±10 m with 0, and seeds 3 and 9 have never learned at ±30 m. `curr125` piles up at ±11.25 m (11–21/40, below the 30 needed to shrink). This is the one-tack geometric limit: a single tack needs the first board out to x ≈ 13 m. Two failure kinds remain: (a) about one seed in four never learns at ±30 m, likely because a 30 m wall punishes the early random drift east that open water forgives; (b) seeds that reach ±10 m and do not recover within 20M.
- 02:40 · `c7_ppo_open`, seeds 0–9: train at ±80 m (≈ open water, the M6 condition) until 30/40, then jump to ±30 m. After that, 2.5 m steps at ≥ 30/40 and back-off after 2 empty panels. 30M decisions.
- 02:51 · `curr25` finished at 20M. Final ±10 m panels by seed: s0 34, s1 40, s2 0, s3 0 (never learned ±30 m), s4 0, s5 36, s6 40, s7 40, s8 40, s9 0 (never learned ±30 m). So 6/10 seeds learn the channel within 20M. `curr125` at 20M: s1 40, s4 34, s2 14, s0 still at ±11.25 m, s3 never learned. `back` at 17–18M: s1 40, s0 4, s2 backed off to ±12.5 m, s4 at ±12.5 m, s3 never learned. `c7_ppo_open` at 4–5M: seeds 6 and 8 have passed the open-water phase. SAC: seed 6 joined the learned set (40/40 since 6M); seeds 5, 7, 8 and 9 are 0 at 9M. `c3_sac_mix` s0 is at 33/40 on ±10 m; s1–4 are 0. Held-out evaluation of `curr25` s3–9 started.
- 02:59 · `c7` ±80 m first phase at 8–9M: only seeds 4, 6 and 8 reached 30/40 there, against 9/10 M6 seeds learning open-water U by 7M. The bounds differ only by the east wall at 80 m instead of 120 m. A beam-reaching early policy drifts that far within 180 s, so the closer wall turns early drift into −10 failures. That is the same pressure that teaches hovering, and it likely explains why a quarter of seeds never learn at ±30 m. The seven `c7` seeds still in phase 1 were stopped; seeds 4, 6 and 8 continue. Added `--open-first-u`: phase 1 is U itself (its frozen bounds), so training is bit-identical to `m6_tune` until the first switch to ±30 m. Launched `c8_ppo_openu`: U first, then ±30 m, 2.5 m steps at ≥ 30/40, back-off after 2, 30M decisions, seeds 0–9.
- 03:00 · Held-out evaluation of all 10 `curr25` seeds at 20M (`heldout/c4_ppo_curr25_s0-2_20M.json`, `_s3-9_20M.json`):
  - s1, s7 and s8: validation 20/20, test 50/50, shift 20/20 each;
  - s6: 20/20, 50/50 and 5/20;
  - s5: 18/20, 39/50 and 3/20;
  - s0: 17/20, 37/50 and 3/20;
  - s2, s3, s4 and s9: 0.

  Totals: validation 115/200, test 276/500, shift 71/200. 6/10 seeds pass, against 0/10 for `c1`.

  `c1` SAC at 10M, all 10 seeds: seeds 1, 3, 4 and 6 learn (40/40 on the panel); seeds 0, 2, 5, 7, 8 and 9 stay at 0. That is 4/10. `c3_sac_mix` at 10M: s0 30/40 on ±10 m, the others 0. Launched `c8_sac_openu` (SAC with the same U-first curriculum, 20M decisions, seeds 0–4, GPU) and the held-out evaluation of `c1` SAC s5–9.
- 03:05 · `c8_ppo_openu` reproduces M6 exactly in phase 1. The U panel at 3M is s0 32, s1 29, s2 40, s5 11 and s6 39, the same as `b9_tgt_k10_h256` in M6. Seed 9 switched to ±30 m after 33/40 at 2M, as M6's seed 9 scored. `c5` and `c6` finished at 25M. On the ±10 m panel: `curr125` s1, s2 and s4 at 40, s0 at 0, s3 never learned; `back` s0, s1 and s2 at 40, s4 stuck at ±12.5 m, s3 never learned. Each is 3/5. Held-out evaluation of both started.
- 03:15 · Held-out evaluations:
  - `c1` SAC s5–9 (`heldout/c1_sac_s5-9_10M.json`): s6 scores 20/20, 50/50 and 13/20; the rest 0. `c1` SAC over 10 seeds is 4/10 passing, with validation 80/200, test 200/500 and shift 62/200.
  - `curr125` (`heldout/c5_c6_25M.json`): s1, s2 and s4 perfect (20/20, 50/50, 20/20). s0 was selected at 15.5M and scores 2/20, 17/50 and 0/20, which passes weakly. s3 is 0. That is 4/5 by the ≥ 1/20 rule.
  - `back`: s0, s1 and s2 perfect; s4 scores 0/20 and 2/50; s3 is 0. That is 3/5.

  `c7` seeds 4, 6 and 8 (past their ±80 m phase) are 40/40 at ±10 m by 20–21M. `c8_ppo_openu` at 10–11M: 9/10 seeds are in the curriculum (±15–30 m, mostly 40/40); s8 is still in U, as M6's s8 never learned. `c8_sac_openu` at 3M: s1 has passed the U phase.
- 03:37 · `c8_ppo_openu` held-out at 30M (`heldout/c8_ppo_openu_30M.json`; selected = final on every seed). Per seed, validation / test / shift:
  - s0: 1/20, 12/50, 0/20;
  - s1: 20/20, 50/50, 16/20;
  - s2: 20/20, 50/50, 0/20;
  - s3: 20/20, 50/50, 15/20;
  - s4: 20/20, 50/50, 10/20;
  - s5: 20/20, 50/50, 16/20;
  - s6: 0, 0, 0;
  - s7: 20/20, 50/50, 20/20;
  - s8: 0, 0, 5/20;
  - s9: 20/20, 50/50, 7/20.

  Totals: validation 141/200, test 362/500, shift 89/200. **8/10 seeds pass and 7/10 are perfect on validation and test.** All 900 episodes replay exactly. Seed 2 counts 0 tacks and seed 9 counts 1, but their tracks show two direction reversals, with legs sailed inside the classifier's 30° zone. Seed 9 also stalls near the gate and comes round again (103 s mean test finish). `c7` seeds 4, 6 and 8 at 30M: 60/60 validation, 150/150 test, 50/60 shift. `c8_sac_openu` at 9M collapses at each width switch: s1 went 40 in U → 0 at ±30 → 40 → 7 at ±22.5 m; s2 and s4 are 0 at ±30 m; s0 and s3 have not left U.
- 03:40 · Report, results (`docs/ese3990/M7_channel_results.json`), figures (`M7_channel_curves.png`, `M7_channel_tracks.png`) and `tests/test_m7_channel.py` (4 tests) written; 9 tests pass with M6's. Launched `c9_ppo_openu40`: `c8` with a 40M budget, seeds 0–9. CPU PPO is deterministic, so its first 30M must reproduce `c8` exactly; the extra 10M tests whether `c8`'s remaining failures are budget-bound.
- 04:17 · `c8_sac_openu` finished at 20M with 0/40 on the ±10 m panel at every checkpoint on all 5 seeds, so no held-out evaluation was run. Seeds 2 and 4 narrowed to ±15 m, then fell to 0 at ±12.5 m. Seed 1 oscillated between ±20 and ±22.5 m. Seeds 0 and 3 never left U. Direct training (`c1`, 4/10) is the better SAC arm. `c9_ppo_openu40` at 31–32M: the first 30M reproduce `c8` exactly on all 10 seeds. Seed 8 reached ±10 m at 31M and immediately scored 40/40; seeds 1, 2, 3, 4, 5, 7 and 9 are at 40, seed 0 at 4, and seed 6 is back at ±30 m with 0.
- 04:27 · `c9_ppo_openu40` held-out at 40M (`heldout/c9_ppo_openu40_40M.json`):
  - seeds 0–5 and 7–9: validation 20/20 and test 50/50 each; shift 13, 16, 20, 14, 20, 8, 20, 9 and 14 of 20;
  - seed 6: 0.

  Totals: **validation 180/200, test 450/500, shift 134/200; 9/10 seeds pass**, all 900 episodes replay exactly. Seed 4 was selected at 39M: its 40M checkpoint dropped to 16/40 on the panel and scores 8/20, 22/50 and 9/20. Seed 0 recovered to 40/40 at 35M, and seed 8 reached ±10 m at 31M with 40/40. Mean test finish is 44–52 s, seed 9 100 s.

## Closed (04:28)

The goal's consistency bar is met for PPO. With the U-first width curriculum and back-off, 9/10 seeds are perfect on validation and test at 40M, matching M6's open-water 180/200 and 450/500 at 4× the budget; shift is 134/200 against 180/200. SAC learns the channel without a curriculum on 4/10 seeds (M6: 4/5), and the SAC variants tried (width mixing, U-first curriculum) did not raise that. SAC is therefore not yet as consistent as M6; its curriculum collapses at width switches. All training processes have exited. `runs/ese3990/M7/train/tmp/tack_stats.py` is a scratch helper. Report: `docs/ese3990/M7_channel_report.md`; results: `docs/ese3990/M7_channel_results.json`. Nothing committed.

## Follow-up · longer channels, zero-shot (after close)

`experiments/ese3990/m7_channel_long.py` runs the `openu40` policies (selected checkpoints) in the ±10 m channel with the gate at 40, 80, 120 and 160 m: 360 s limit, top bound at gate + 60 m, U test conditions. Results are in `runs/ese3990/M7/long/results.json` and `results_relay40.json`.

- As trained, 10 seeds: 450/500, 36/500, 0/500 and 0/500. Policies key their tacks to the gate offset, which never exceeded 40 m in training. Most tack once at the right wall and then never make the next tack; some never tack.
- With `--relay 40` (a virtual gate every 40 m, advanced on passing): 450/500, 322/500, 246/500 and 210/500. Seeds 1 and 8 score 46/50 at 160 m with 8 tacks; seed 7 scores 50/50 at 80 m.
- Figure: `docs/ese3990/figs/M7_channel_long_tracks.png`; report section "Longer channels".
