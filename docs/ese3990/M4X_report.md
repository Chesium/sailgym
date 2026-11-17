# M4X — Explain the upwind failures; let learning shorten planning (exploratory)

**Status: closed early, 2026-11-17 (user decision).** The held-out stage was
stopped, and its audit and the generated report files were skipped; see
[Not done](#not-done-skipped-2026-11-17). Every M4X row is
**post-hoc and exploratory**. U's validation outcomes were already open when
M4X was designed (plan §6), so none of these rows is merged into an M4/M4U
table, and no M4/M4U freeze, row or store was changed. Three arms ran, each
differing from its M4U parent in one declared change:

- **X1:** SAC + demonstrations + critic LayerNorm.
- **X1b:** SAC + critic LayerNorm, from scratch.
- **X2:** CEM-MPC H=40 + a learned value tail.

**Verdicts.**

- **X1 and X1b: the critic is still unbounded.** All 6 runs fail the declared
  rule and the entropy-adjusted rule. LayerNorm does not stabilize SAC at
  γ = 0.99975, and X1 finishes no more often than its parent.
- **X2: never tacks, like H=40, and it exploits V.** It finishes 0/60 on
  validation. It sails away from the gate into states that V overvalues.

## Frozen protocol

[`configs/M4X_freeze.json`](../../experiments/ese3990/configs/M4X_freeze.json)
was written on 2026-11-16 at 21:07 UTC, before any X run. It records:

- the arms, seeds [0, 1, 2] and the two learner configs (each is the M4U
  config plus `critic_layernorm: true`);
- X2's planner (H = 40, K = 32, I = 3, 0.5 s blocks, the M3 settings) and its
  three V checkpoints, with hashes;
- the thresholds, the latency conditions and the declared readings;
- the cut order;
- the SHA-256 of all 13 sources the runs execute (`m4x.py` refuses to run if
  any of them changes);
- the V-check file.

[`M4X_freeze_amendment1.json`](../../experiments/ese3990/configs/M4X_freeze_amendment1.json)
fixes the entropy-adjusted critic rule in its final form. It was written before
any X1 run and before any M4X result was read (see
[Critic scale](#critic-scale)). The plan carries the matching dated line.

**What changed in the code, all default-off.** Every M2/M4/M4U config and
checkpoint reloads unchanged.

- `sac.py`: optional LayerNorm (learned scale and offset, eps 1e-6) between
  the Dense and the tanh of each hidden layer of both critics and their Polyak
  targets, in RLPD's order (Ball et al., 2023). Random draws are unchanged, so
  each X run starts from its M4U parent's exact initial actor. Initial
  validation returns match the parents: −5.28, −5.49 and −5.54 for seeds 0, 1
  and 2.
- `train.py`: a `--critic-layernorm` flag, accepted for SAC on U only. New runs
  also log the per-window maximum and minimum Q (`q_max`, `q_min`).
- `mpc.py`: `CEMMPC(tail=V)` adds γ^H·V(φ(s_H)) only to candidates still
  running after H decisions. No tail is added after a finish, failure or the
  time limit. It records the best predicted score at every decision.
- Unit tests in `tests/test_sac.py`, `tests/test_mpc.py` and
  `tests/test_m4x.py` cover:
  - LN off by default with identical draws;
  - LN invariance to pre-activation scale and shift;
  - LN gradients, Polyak averaging and reload;
  - the tail added only for candidates still running at H;
  - the X2 group replaying exactly and resuming;
  - refusal of changed sources;
  - the amendment rule.

## V check

V is the critic of PPO + BC's initial checkpoint for the same seed (M4U). It
was fitted by regression to the 50 demonstrations' discounted returns-to-go
and is not refitted. The check uses RuleSailor's 20 U validation episodes,
which V never saw ([`M4X_vcheck.json`](M4X_vcheck.json)). The constant
predictor is the mean of V's own fitting targets, +3.51, with MSE 55.7.

| Seed | MSE / constant MSE | Correlation | Fit MSE on the demonstrations |
|---|---|---|---|
| 0 | 0.81 | 0.42 | 13.2 |
| 1 | 0.76 | 0.48 | 14.0 |
| 2 | 1.03 | 0.27 | 13.7 |

V is weak away from its training data. Seed 2 is no better than a constant.
This was reported before any X2 run and, as declared, V was not refitted on
planner outcomes.

## Critic scale

*(Figure not generated: `m4x_report.py --out docs/ese3990` was skipped; see [Not done](#not-done-skipped-2026-11-17).)*

SAC's critic estimates a *soft* value, which includes the discounted entropy
term o = α·H·γ/(1−γ). At U's γ = 0.99975 that term is multiplied by about 4,000:
with H at its −2 target, an accurate critic reads about task return − 8,000·α.

- **Declared rule:** max |Q| ≤ 250 and mean Q ≤ +20 throughout the run. It
  cannot tell a blow-up from a large α.
- **Adjusted rule (amendment 1),** declared before any result:
  - From 400k decisions on, [lo, hi] = [min(0, o), max(0, o)] over every window
    since 400k.
  - Bounded if q_max − hi ≤ 250, lo − q_min ≤ 250 and mean Q − hi ≤ +20 at
    every window.
  - It admits a critic that lags anywhere between 0 and the offsets seen. The
    first version, which subtracted the current window's o, called such a lag a
    blow-up and was replaced before use.
- The readings use the adjusted rule.

The feasible discounted task return on U is [−22.3, +15.7]. On R it is
[−16.6, +15.0], and all three R runs are bounded under both rules.

| Arm (U) | Seed | max \|Q\| | Final mean Q | Final α | [lo, hi] since 400k | Worst excess beyond [lo, hi] | Declared | Adjusted |
|---|---|---|---|---|---|---|---|---|
| SAC (M4U) | 0 / 1 / 2 | 4,166 / 2,223 / 5,040 | −3,631 / −1,822 / −3,910 | 0.096 / 0.055 / 0.018 | lo −811 / −452 / −1,098 | spread ≥ 3,355 / 1,772 / 3,942¹ | unbounded | unbounded |
| SAC + demos (M4U, expert data) | 0 / 1 / 2 | 6,934 / 8,499 / 9,062 | +1,817 / +743 / +3,314 | 0.178 / 0.469 / 0.362 | lo −8,886 / −11,153 / −11,718 | mean +1,817 / +748 / +3,323 | unbounded | unbounded |
| **X1b** SAC + LN | 0 / 1 / 2 | 6,350 / 7,353 / 8,699 | −5,694 / −6,442 / −6,823 | 0.088 / 0.030 / 0.034 | lo −913 / −1,074 / −1,435 | q_min below lo by 5,436 / 6,279 / 7,264 | unbounded | **unbounded** |
| **X1** SAC + demos + LN | 0 / 1 / 2 | 5,014 / 3,676 / 5,035 | −3,873 / −2,344 / −3,832 | 0.457 / 0.383 / 0.349 | lo −4,146 / −3,438 / −3,523 | q_max above hi by 2,814 / 2,082 / 2,290 | unbounded | **unbounded** |

¹ M4U logged only max |Q| per window, so the spread excess is bounded from both
sides. Its lower bound already exceeds 250.

What LayerNorm changed:

- **X1 versus its parent:**
  - max |Q| falls from 6,934–9,062 to 3,676–5,035.
  - Mean Q stays inside the band (mean − hi ≤ −78) and no longer flips positive
    late in the run.
  - But within the batch the critic's spread is thousands. q_max sits about
    +2,000 above hi = 0 while mean Q is near −3,000. The first violation comes
    at 401k–766k decisions, almost as soon as the rule applies.
- **X1b versus its parent:**
  - LN makes the critic worse: max |Q| is 6,350–8,699 against 2,223–5,040.
  - Mean Q drifts to between −5,700 and −6,800, while the offset is only −233
    to −728.
  - q_min falls 5,436–7,264 below lo, with the first violation at 1.36M–1.65M
    decisions.

**Why LayerNorm could not bound Q here.** The critic is 2 × 64 **tanh**, not
RLPD's ReLU. Its hidden features are already bounded, at ‖h‖ ≤ 8 for 64
units, with or without LN. LN only rescales each pre-activation. |Q| can grow
only through the linear output layer, which LN does not touch. In a ReLU
network LN bounds the feature norm; in this one it bounds nothing new. This
explains, after the fact, why the RLPD component did not transfer. It is not a
tested claim.

## X1/X1b: training-time validation

*(Figure not generated: `m4x_report.py --out docs/ese3990` was skipped; see [Not done](#not-done-skipped-2026-11-17).)*

Validation uses 20 U episodes at each checkpoint (M4U's protocol) and reports
finishes out of 20.

| Run | Initial | 400k | 1M | 2M | 3M | 4M | Selected |
|---|---|---|---|---|---|---|---|
| X1 s0 | 0 | 0 | 0 | 0 | 0 | 0 | initial (93k) |
| X1 s1 | 0 | 1 | 0 | 0 | 0 | 0 | 400k |
| X1 s2 | 0 | 1 | 0 | 0 | 0 | 0 | 400k |
| SAC + demos s0 (M4U parent) | 0 | 1 | **7** | 0 | 0 | 0 | 1M |
| SAC + demos s1 (M4U parent) | 0 | 1 | 0 | 1 | 0 | 1 | 400k |
| SAC + demos s2 (M4U parent) | 0 | 0 | 0 | 0 | 0 | 0 | 2M |
| X1b s0 / s1 / s2 | 0 | 0 | 0 | 0 | 0 | 0 | 4M / 2M / 3M |

- Checkpoints were selected by M4U's validation-only rule: most finishes, then
  highest mean return, then earliest.
- X1 never exceeds 1/20 and does not reproduce the parent's 7/20 peak.
- X1b, like SAC from scratch, never finishes.
- Mean validation returns stay between −5.2 and −9.0, the timeout regime (a
  timeout from the start returns about −5.3).

**Held-out (partial and unaudited, exploratory).** All 36 X1/X1b checkpoint
groups completed. These stores were never replay-audited, so treat the counts
as indicative.

| Arm / role | Test finishes | Shift finishes | Main outcomes on test |
|---|---|---|---|
| X1 selected | 1/150 | 0/60 | 121 timeouts, 17 out of bounds, 11 mark missed |
| X1 final | 3/150 | 0/60 | 117 timeouts, 17 out of bounds, 12 mark missed, 1 capsize |
| X1b selected | 0/150 | 0/60 | 150 timeouts |
| X1b final | 0/150 | 0/60 | 148 timeouts, 2 out of bounds |
| *SAC + demos (M4U parent), selected / final* | *5/150 / 0/150* | *0/60 / 0/60* | |

## X2: CEM-MPC H=40 + value tail

Validation ran alone on the machine: 3 jobs × 4 Rayon threads, 05:08–05:50
+08:00. It covers 60 episodes (3 search seeds × 20), all stored and replayed
exactly.

| Controller (U validation) | Finishes | Outcomes | Episodes with a tack | Mean max y (m) | Timeout final distance, median (m) | Latency p50 / p95 / max (ms) | Over 50 ms |
|---|---|---|---|---|---|---|---|
| **X2** H=40 + V tail (exploratory) | **0/60** | 22 out of bounds, 11 mark missed, 27 timeouts | **0/60** | 13.9 | 108.3 | 38.3 / 63.2 / 130.8 | 18.1% |
| MPC H=40 (M4U sweep) | 0/60 | 59 timeouts, 1 out of bounds | 0/60 | — | 65–74 | 47.8 / 82.4 / 152.9² | 43.7%² |
| MPC H=160 (M4U sweep) | 60/60 | 60 finishes | — | — | — | 193 p50² | 96.4%² |

² M4U's sweep shared the CPU with 12 training runs, so its latencies are not
like-for-like with X2's.

- **Latency:** the tail costs 1.5% of decision latency. X2's p50 of 38 ms
  meets the real-time p50 < 50 ms test, but 18% of decisions miss the 50 ms
  deadline.
- **Predicted versus executed return** (gap = predicted − executed, over all
  171,693 decisions):
  - mean +12.6, median +13.0, RMSE 13.4, correlation 0.12;
  - at the episode start, X2 expects +3.5 and receives −5.8.
- **V along the executed tracks** (from an inline analysis of the same
  stores; the `m4x_report.py` code that would regenerate it never ran): the per-episode
  maximum of V averaged +10.3 / +9.7 / +12.1 by seed. That exceeds RuleSailor's
  best validation start return, +7.45. X2 sails downwind, away from the gate at
  y = 40 m: its mean maximum y is 13.9 m. It is heading for states that V
  overrates.

*(Figure not generated: `m4x_report.py --out docs/ese3990` was skipped; see [Not done](#not-done-skipped-2026-11-17).)*

**Held-out X2 (partial and unaudited, exploratory).** The run stopped at 207
of 213 episodes, with 6 shift episodes missing (search seeds 0 and 2):

- **Finishes:** 0/3 nominal, **0/150 test**, **0/54 shift**.
- **Test outcomes:** 56 mark missed, 44 out of bounds, 50 timeouts.
- **Shift outcomes:** 14 out of bounds, 5 mark missed, 35 timeouts.
- **Latency:** p50 / p95 / max 38.9 / 60.3 / 145.1 ms; 16.7% of decisions over
  50 ms; the tail costs 1.5% of latency.

This matches validation: no finishes, and failures more often than timeouts.

## Which declared reading applies

| Observation (declared) | Applies? | Evidence |
|---|---|---|
| X1 bounded, and finishes > SAC + demonstrations | **No** | X1 is unbounded under both rules |
| X1 bounded, finishes unchanged | **No** | Unbounded |
| X1 still unbounded → LayerNorm alone does not stabilize SAC at γ = 0.99975; report it, with no further fixes in M4X | **Yes** | 3/3 seeds: q_max reaches 2,082–2,814 above the adjusted band from 401k–766k decisions on. X1b is unbounded too, from the other side. No further fixes were tried |
| X2 finishes ≈ H=160 in real time | **No** | 0/60 against 60/60 |
| X2 tacks but finishes less often than H=160 | **No** | No episode tacks |
| X2 never tacks, like H=40 → V does not encode the worth of a tack; the horizon gap remains | **Yes** | 0/60 episodes with a tack. Worse than plain H=40: 33 failures against 1. The planner is optimistic by +12.6 on average and steers into states where V exceeds any RuleSailor start return |

## Scope and limitations

- **Exploratory and post-hoc.** U's outcomes were open when M4X was designed.
  Seeds are training or search seeds; there are no episode-level intervals.
- **One change per arm.** No γ, α or learning-rate sweep, no LN for PPO and no
  refitting of V, as declared.
- **The tanh argument is an explanation, not a test.** No ReLU critic was run.
- **V is one fit.** V comes from 50 demonstrations with no bootstrapping. A V
  fitted on wider data (for example on planner or RL rollouts) might behave
  differently. It is excluded here by the plan.
- **Latency was measured on a shared multi-process machine** (3 concurrent
  jobs). Absolute milliseconds depend on load. M4U's sweep was measured under
  heavier load.
- **The extension was swapped mid-run (08:44, held-out stage).** A command run
  without `--frozen` re-synced the virtual environment and replaced the Rust
  extension on disk with an older cached build. The three running workers kept
  the original in memory: its segment layout matches the restored build. It was
  restored 2 min later (08:46:50), with the identity rechecked against the freeze.
  The replay audit that would prove the held-out stores exact was skipped, so
  the held-out counts above are unaudited.

## Not done (skipped 2026-11-17)

M4X was closed early by user decision, to start M4P. This is what was left
undone:

- **Held-out stage:** stopped at 10:58 +08:00. X2 reached 207 of 213 episodes:
  6 shift episodes are missing for search seeds 0 and 2. All 2,556 learner
  episodes are complete. There is no `complete.json`, so
  `m4x_report.py`'s held-out path skips the stage.
- **Replay audit:** not run on `runs/ese3990/M4X/x2_validation` or `heldout`.
  The validation stores replayed during the earlier report run; the held-out
  stores never did, and they were written across the 08:44 extension swap.
- **Generated outputs not produced:**
  - `m4x_report.py --out docs/ese3990` never ran on final data, so the three
    figures, `M4X_results.json` and `M4X_outcomes.md` are missing.
  - The new "V along the executed tracks" code never ran.
  - The held-out code path of `m4x_report.py` is untested on real data.
- **Diagnostics:** no held-out tack or irons diagnostics; they need the
  exact-replay pass.
- **Claims:** no `M4X_claims.json` claim-index entries.

None of these changes the declared readings. Those rest on the critic-scale
verdicts and on validation, both of which are complete.
