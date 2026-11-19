# M6 tuning campaign · from-scratch PPO/SAC on U without demonstrations

Started November 19, 2026 at 16:15 (+08:00) under the user's 8-hour goal: improve the existing PPO and SAC so they learn to tack and sail upwind on U without expert demonstrations, iterating on pilots. Exploratory and post-hoc. No frozen source, task, reward, condition manifest or earlier store is changed. New files only.

## Harness

- `experiments/ese3990/m6_tune.py`: vectorized trainer (16 envs) that reuses `ppo.py`/`sac.py` losses and updates, `TaskEnv`, `TransitionReward` and the training wind stream. Rust receives the same 20 Hz rate actions with release off. Switchable training-side factors: feature scaling (`--norm`), action head (`--head`), action repeat (`--repeat`), random starts (`--p-random`), mirror-equivariant networks (`--equivariant`), training-only failure penalty (`--train-fail`), LR annealing (`--anneal`), KL early stop (`--target-kl`).
- Pilot score: deterministic policy, original (0, 0) start, 40 fresh training-distribution conditions (`TrainingSampler(90000)`). Validation, test and shift stay closed during tuning.
- `m6_tune_eval.py`: stored, exactly replayed episodes plus the frozen M4U tack diagnostics for selected checkpoints. `m6_tune_plot.py`: track plots. `m6_tune_report.py`: run table.
- Tests: `experiments/ese3990/tests/test_m6_tune.py` checks that vectorized steps match `TaskEnv.step` bit for bit, the target head reaches its target, GAE matches `ppo.gae` at repeat 1, U is mirror-symmetric to float precision, and the equivariant nets are exactly equivariant.
- Runs: `runs/ese3990/M6/tune/<name>_s<seed>/` (`args.json`, `log.jsonl`, checkpoints every 1M decisions). Launch with `runs/ese3990/M6/tune/launch.sh NAME SEED [args]`.

## Findings so far (16:35)

1. **The frozen calibration normalization is a blocker.** M4U-style statistics come from 10k smooth-random decisions near the start: `cos_psi` std 0.14 and `dy_target` std 6.6 for seed 1, so starboard-tack headings and near-gate positions clip at ±5. With fixed physical scales, PPO climbs to y ≈ 30–40 m within 1–3M decisions. With calibration scaling it stays near the start (max y 0–14 m at 2–3M).
2. **Position-target action head.** The policy outputs a rudder angle and sheet length, and a dead-beat inner loop converts them to rates from the observed actuator state. Combined with fixed scaling, it gave the first original-start finishes: 18/40 at 3M (20 Hz, seed 1), 15/40 at 4M (k = 5, seed 1) and 32/40 at 7M (k = 10, seed 1). Every finish in the replayed 18/40 check had exactly one settled tack.
3. **Instability / timeout trap.** Runs swing between attempting and timing out, for example 15/40 → 0/40 between 4M and 5M. Most stochastic training episodes time out even when the deterministic policy finishes, which points to exploration noise on the held targets.
4. Random starts (50% in x ∈ [−30, 30], y ∈ [0, 38]) hurt original-start progress in both heads through 4M.

## Batch 1 at 10M decisions (16:38)

Finishes out of 40 at each 1M checkpoint (deterministic, original start):

| Run | 0–10M | Note |
|---|---|---|
| rate head, calibration scaling, 20 Hz, s0 | 0 ×7 (stopped at 7M) | the M4U setup, vectorized; max y ≈ 0 |
| rate head, fixed scaling, k = 10, s0 | 0 0 0 0 0 0 0 1 2 7 19 | rising |
| rate head, fixed scaling, k = 10, s1 | 0 0 0 0 0 0 0 0 9 8 19 | rising |
| target head, fixed, k = 10, s0 | 0 ×11 | max y 28 |
| target head, fixed, k = 10, s1 | 0 0 0 0 6 1 17 32 24 7 40 | **200/200** on `panel200`, every finish one settled tack, mean 75 s, 200/200 replay exactly |
| target head, fixed, k = 5, s0 / s1 | 0 ×11 / one 15 at 4M then 0 | unstable |
| target head, fixed, 20 Hz, s1 | 0 0 0 18 (stopped early by mistake) | relaunched as `b3_tgt_k1` |
| λ = 0.99 (target, k = 5), 2 seeds | 0 | |
| 50% random starts (both heads, k = 5) | 0 | |

Reading: 0.5 s decisions (M5's B factor) plus fixed feature scaling learn the tack from scratch. M5's B cell used calibration scaling and never finished. `b3_rate_calib_k10` is the attribution control.

## Batch 2–3 (16:20–16:49)

- **Target head, fixed scaling, k = 10, seeds 0–4** (finishes /40 per 1M): s0 never; s1 peaks at 40; s2 17 at 3M then 0 to 10M (collapse); s3 30, 17 at 9–10M; s4 22 35 25 40 40 40 40 from 4M with 988/989 training finishes. Four of five seeds learn the tack at some point; two hold it.
- **Collapse mechanism (s2):** at 3M the deterministic policy finished 17/40 while the noisy training policy finished 1/287 and missed the marks 30 times. Near-gate experience is then dominated by −10 misses and the policy retreats into timeouts (≈ −4).
- **Mirror-equivariant networks** (exact, via symmetrization): no benefit. Target k = 10: 0/40 on both seeds through 10M. Target k = 5: max 1/40. Rate k = 10: s0 6, 19 then 0. Dropped.
- **20 Hz target head** reproduced 18/40 at 3M exactly, then 0 through 8M. Dropped for cost and instability.
- Lower initial noise (log σ = −1.0, target k = 10): s1 12 4 9 27 by 7M, s0 0.
- SAC (k = 10, fixed scaling, one update per policy step): α falls to 0.001 by 1M decisions and entropy sits below the −2 target. No finish at 1M.

## Batch 3–4 at 20M (17:08)

- **Attribution control:** rate head, *calibration* scaling, k = 10, seeds 0–1: 0/40 at every checkpoint through 20M (max y 35–38 at the end). Same config with fixed scaling: seeds 0, 1, 4 finish (19, 19 at 10M; s4 40/40 from 12M to 20M), seeds 2, 3 never.
- **Once learned, it holds.** Rate head s4: nine consecutive 40/40 checkpoints to 20M, 848/848 training finishes in the last 1M. Target head log σ = −1.0 s1: eleven consecutive 40/40 to 20M, 1265/1265 training finishes. Rate head log σ = −1.5 s0: 40/40 from 7M to 13M (stopped). Target head ent 0 s2: 6 28 22 40 40.
- **Failure mode of stuck seeds:** the deterministic policy sails NE, heads up into the wind and falls back on the same side again and again. It never completes a tack and times out at y ≈ 15–30. No training episode crosses the line, so there is no finish or miss to learn from.
- **Training-only failure penalty 3:** ~1,200 episodes per 1M, all ending in early failures and none finishing. A cheap failure beats paying the time cost, so the penalty ordering in the frozen reward is doing real work. Stopped.
- LR 1e-4 and anneal + KL early stop: no finish by 6M / 18M. Stopped.

## Held-out check at a common 10M budget (17:18)

Checkpoints were selected on the pilot panel only (most finishes, ties to the later one). Each was then scored on the frozen validation, test and shift manifests from the original start, with stored episodes that all replay exactly. Files: `runs/ese3990/M6/tune/heldout/{rate,tgt}_k10_10M.json`.

| Config (fixed scaling, k = 10, 10M) | Validation /20 by seed 0–4 | Test /50 | Shift /20 | Seeds ≥ 1/20 val |
|---|---|---|---|---|
| Rate head (original action semantics) | 13, 8, 0, 0, 8 | 15, 20, 0, 0, 20 | 3, 0, 0, 0, 10 | 3/5 |
| Target head | 0, 20, 3, 17, 20 | 0, 50, 23, 40, 50 | 0, 20, 8, 3, 20 | 4/5 |

Both configs meet the pre-declared M5 threshold (≥ 3/5 seeds with ≥ 1/20 validation finishes). Every from-scratch PPO/SAC row in M4U/M5/M5b/M6 scored zero against it. Reference rows from the M4U report: RuleSailor 38/50 test; MPC H = 160 (offline, ~193 ms per decision) 150/150 test.

Hard seeds (0, 2 in the target head; 2, 3 in the rate head) stayed stuck under pink noise (β = 1), initial log σ = 0 and 64 × 64 env batches through 8–12M.

SAC: target head, k = 10, 256 hidden units on the GPU (scratchpad venv, see below), seed 0: 0 0 0 9 by 3M, with 42/307 training finishes. The CPU 64-unit SAC and the rate-head 256-unit SAC have no finish yet.

GPU JAX: a separate venv in the session scratchpad (`gpuenv`, `jax[cuda13]==0.11.2`, optax and numpy pinned to the project versions). A `.pth` file adds the project `.venv` site-packages for `sailgym_core`. The project `.venv` is untouched. Launch with `runs/ese3990/M6/tune/launch_gpu.sh`. The scratchpad can be cleared; recreate the venv with `uv venv` + `uv pip install "jax[cuda13]==0.11.2" optax==0.2.9 numpy==2.5.3`.

## Width (17:27): 2 × 64 was a bottleneck

The 2 × 64 actor/critic width is frozen in plan §4.5. Widening both PPO networks to 2 × 256 cracked the seeds that never learned at 64:

- Target head, k = 10, s0: 0 0 2 32 40 40 by 5M (stuck at 0 through 20M in every 64-unit variant).
- Target head, k = 10, s2: 0 0 0 40 31 40 by 5M (64-unit: 17 at 3M, then 0 to 10M; 0 under pink noise, log σ = −1.5 and 64 envs).
- Rate head, k = 10, s3: 0 0 0 18 5 24 by 5M (64-unit: 0 through 20M).
- **SAC**, target head, k = 10, 256 units, s0: 0 0 0 9 33 38 by 5M, with 656/681 training finishes in the 5th million. The 64-unit SAC (CPU) has 0/40 through 9M on both heads, with α ≈ 0.001 and Q drifting to −6.

Other 17:10–17:27 results, all on hard seeds: pink noise (β = 1) none; log σ = 0 none; 64 envs × 64 steps: rate s0 discovered at 8M and stayed unstable (≤ 30); λ = 0.98 none by 6M; k = 20: rate s0/s1/s4 discover but oscillate (≤ 35); target s0 17 33 34 38 40 40; target s2 one 9.
SAC n-step 8 + target entropy 0 (rate and target), target entropy 0 alone, LayerNorm critics, and the rate-head 256-unit SAC: none by 3–5M.

Next: 256-unit seeds 0–4 for PPO (both heads) and SAC (target head). Controls at 256 units: calibration scaling (target, k = 10, s0 and s2) and 20 Hz (target, s0).

## Final PPO configuration and 10-seed held-out result (17:50)

Settings: `--head target --repeat 10 --hidden 256`, fixed scaling. Everything else is at harness defaults: 16 envs × 256 policy steps, minibatch 512, 4 epochs, lr 3e-4, clip 0.2, λ 0.95, entropy 0.001, initial log σ −0.5. Seeds 0–9. Seeds 0–4 ran to 12–13M and were stopped; seeds 5–9 ran to 10M.

- Pilot panel: 9/10 seeds reach 40/40 between 3M and 9M and stay there. Seed 8 is 0 through 10M (max y 7).
- Held-out, panel-selected at ≤ 10M: 180/200 validation, 450/500 test, 180/200 shift. Seeds 0–7 and 9 are 20/20, 50/50, 20/20; seed 8 is 0. Every finish has one settled tack. Mean test finish 39.4–40.2 s, 47.3 s for seed 4.
- Held-out at ≤ 4M (main budget): 110/200 validation, 293/500 test, 66/200 shift. Seeds with ≥ 1/20 validation: 7/10.
- All 1,800 stored held-out episodes replay exactly. Files: `runs/ese3990/M6/tune/heldout/tgt_k10_h256*_{4M,10M}.json`.
- Report: `docs/ese3990/M6_tune_report.md`. Results JSON: `docs/ese3990/M6_tune_results.json` (regenerate with `m6_tune_results.py`). The plan has a new "M6 tuning" subsection and a status-line sentence.

Controls at 256 units: calibration scaling, target head, k = 10: s2 learns (40 at 12M), s0 0 through 12M. Original M4U setup with only the width changed (calibration, rate, 20 Hz): 0/2 through 10M. Target head at 20 Hz: s0 0 through 11M.

## SAC status (17:55)

Target head, k = 10, 256 units (GPU): s0 40/40 from 7M to 10M; s2 0 0 0 2 0 1 4 11 at 7M; s3 28 at 5M; s1 and s4 0 at 5–7M. Queued on the GPU: UTD 2 (s1, s2) and pink collection noise (`--noise-beta 1`, s1, s4). Target entropy −1 was stopped at 1M to make room.

## Stress test, figures, SAC queue (18:03)

- Stress split (`m6_tune_eval.py ... stress`): 32 conditions outside every frozen range (wind 3.5/4/6/7 m/s × direction ±10/±15° × heading 60/120°). The nine converged PPO policies finished 211/288 and all 288 replay exactly. By factor: direction −15° 26/72, −10° 55/72, +10° 68/72, +15° 62/72; wind 3.5 m/s 46/72, 7 m/s 44/72. The one-tack port-first plan breaks when the wind backs.
- Figures: `docs/ese3990/figs/M6_tune_{curves,tracks}.png` from `m6_tune_figs.py`.
- Lint: `ruff format` + `ruff check` clean on all `m6_tune*` files and the test. The reformatted trainer reproduces `b9_tgt_k10_h256_s0`'s first two log rows exactly.
- SAC queue: seeds 5–9 on CPU (`b5_sac_tgt_h256_s{5..9}`, ~12 min per 1M); 20M reruns of seeds 1 and 4 on GPU (`b14_*`); UTD 2 (`b11_*`, s1/s2) and pink noise (`b13_*`, s1/s4) on GPU.
- PPO open questions still running: 20 Hz with λ = 0.995 (`b12_tgt_k1_h256_lam995`, s0–2), and the rate head at 256 units with log σ −1.5 (`b12_rate_k10_h256_ls15`, s0/2/4).

## SAC held-out and remaining runs (18:27)

- SAC, target head, k = 10, 256 units, seeds 0–4. At 10M: validation 73/100, test 183/250, shift 27/100, 4/5 seeds pass (s4 0). At 4M: 17/100, 45/250, 0/100, 1/5 seeds. Files: `heldout/sac_tgt_h256_*`.
- GPU SAC is not bitwise reproducible. `b14_sac_tgt_h256_20M_s4` (same settings, longer budget) reached 40/40 at 3M, while `b5_..._s4` had none through 10M. Both 20M reruns were stopped at 3M.
- UTD 2: s2 40/40 by 4M (UTD 1 needed 9–10M); s1 0 by 5M. Launched for s0, s3 and s4 at 18:24.
- Pink-noise SAC: s1 5/40, s4 0/40 at 5M. Stopped.
- PPO, 20 Hz with λ = 0.995 (target head, 256 units): 0/40 on all 3 seeds through 10M. The 0.5 s interval is needed.
- PPO, rate head, 256 units, log σ −1.5: s0 40, s2 13, s4 40 at 10M; these seeds failed at log σ −0.5. s1 and s3 launched at 18:24 to complete five seeds.
- Report and plan updated with the SAC held-out rows.

## Rate head with low noise (18:37)

PPO, rate head (original action semantics), fixed scaling, k = 10, 256 units, initial log σ −1.5, seeds 0–4. Pilot curves: s0 40 from 9M; s1 40 from 6M; s2 13 at 10M; s3 13–14 at 7–9M; s4 40 at 9–10M. Held-out at ≤ 10M, panel-selected: validation 82/100, test 195/250, shift 65/100, 5/5 seeds pass (`heldout/rate_k10_h256_ls15_10M.json`). The target head is therefore not required. With the original rate actions, the needed changes are fixed scaling, 0.5 s decisions, 256 units and lower initial noise. Report and plan updated; `M6_tune_results.json` regenerated (135 runs, 10 held-out groups).

## Closed (18:59)

- SAC UTD 2, seeds 0–4: s2 40/40 from 4M; s0, s3 and s4 0 at 6M; s1 0 through 10M. No reliability gain over UTD 1, and GPU nondeterminism adds run-to-run noise.
- All runs stopped at 18:59. Stopped mid-budget: UTD-2 s0, s3, s4 at 6M, and CPU SAC seeds 5–9 at 3M (s6 5/40; the others 0). Their logs and checkpoints are kept.
- Total wall time: 16:15–18:59 (2 h 44 min) of the 8-hour allowance. The campaign closed early because both learners learn U from scratch with replicated held-out results: PPO 9/10 seeds perfect at 10M, rate-head PPO 5/5 passing, SAC 4/5 passing.
- Not done: a 10-seed SAC estimate; SAC shift robustness (27/100); the asymmetric wind-backing failure (stress test); a factorial attribution. These are the natural next experiments.
- Nothing is committed. All new files are untracked on branch `ese3990`. `runs/` is git-ignored, about 0.5 GB under `runs/ese3990/M6/tune`.
- To stop runs safely, use `pkill -f "m6_tune[.]py "` (bracketed, with a trailing space), so the pattern cannot match the calling shell or `test_m6_tune.py`.
