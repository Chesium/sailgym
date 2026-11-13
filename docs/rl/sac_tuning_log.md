# SAC pilot log: task R

The existing agent's runs are retained unchanged under
`runs/ese3990/sac/pilots/S1_base_s100` through `S1_base_s111`.
All use **one configuration**, the plan §4.5 baseline, with 1M real decisions
including 10k calibration, pilot seeds 100–111, and 12 nominal/validation
checkpoint evaluations. No alternative SAC configuration was tried.

**Protocol deviation:** these are 12M pilot decisions, exceeding the plan's
three configurations × 100k allowance. PPO also exceeded that allowance
([PPO log](ppo_tuning_log.md)). These unequal pilot resources prevent a claim
of equal tuning cost. No further tuning was done in M2; the original SAC and
PPO baseline settings were frozen for the main comparison. This records the
existing work; it does not retroactively change the planned allowance.

The selected checkpoint maximizes validation finishes, then validation mean
return, then prefers earlier decisions (`evaluate.select_checkpoint`), just
as for PPO. Its validation score is a selection score. Final/selected scores
below are not held-out results. All failed seeds and checkpoints remain.

| Pilot seed | Final validation | Selected validation | Selected decisions | Final alpha | Training finish / miss / stored episodes | Wall s |
|---|---|---|---|---|---|---|
| 100 | 13/20 | 20/20 | 300,000 | 0.00190 | 1682 / 107 / 2100 | 3236 |
| 101 | 17/20 | 19/20 | 700,000 | 0.00117 | 1331 / 92 / 1848 | 3226 |
| 102 | 12/20 | 15/20 | 800,000 | 0.00185 | 664 / 58 / 1424 | 3223 |
| 103 | 18/20 | 20/20 | 400,000 | 0.00150 | 1605 / 42 / 2023 | 3230 |
| 104 | 20/20 | 20/20 | 1,000,000 | 0.00033 | 1091 / 66 / 1705 | 3219 |
| 105 | 5/20 | 11/20 | 600,000 | 0.00237 | 159 / 67 / 1173 | 3237 |
| 106 | 19/20 | 20/20 | 500,000 | 0.00088 | 1644 / 141 / 2092 | 3222 |
| 107 | 18/20 | 20/20 | 500,000 | 0.00137 | 1394 / 141 / 1916 | 3228 |
| 108 | 13/20 | 20/20 | 400,000 | 0.00127 | 1521 / 57 / 1980 | 3222 |
| 109 | 18/20 | 18/20 | 600,000 | 0.00088 | 1143 / 68 / 1719 | 3229 |
| 110 | 20/20 | 20/20 | 900,000 | 0.00082 | 1558 / 88 / 2013 | 3211 |
| 111 | 15/20 | 18/20 | 300,000 | 0.00082 | 1362 / 30 / 1882 | 3235 |

Stored training episodes include a partial final episode when present.

At 100k, mean validation success is 13.3%; at 500k, 76.3%; at 1M, 78.3%
(range 25–100%). Selected validation success averages 92.1% (55–100%).
Seeds 100 and 108 fall from 20/20 at an earlier checkpoint to 13/20 at 1M;
seed 105 remains weak (5/20 final, 11/20 selected). More updates do not
consistently improve control.

Final window-mean alpha is 0.00033–0.00237, entropy is near the target −2,
and sampled-action saturation is 9.2–12.9%. Negative differential entropy is
valid for continuous actions; it is not the entropy of a discrete distribution.
The largest logged sampled absolute Q is 43.46 across all pilots. Metrics
are finite, but that alone does not establish calibration of Q or good control.

**Costs:** 12M calibration/training decisions plus 1,883,275 recorded
nominal/validation decisions. Each run took 3,211–3,237 s on CPU JAX while
the twelve ran concurrently (about 54 minutes elapsed for the batch;
38,718 s summed per-run elapsed time, not CPU time). Separate SAC component
timers can charge asynchronous JAX work to the next collection call; use total
wall time for budgeting. This is below the scheduling gate of two hours per
500k, so the common main budget is 1M. Costs are measurements on this machine,
not a standalone speed comparison with the differently scheduled PPO pilots.

**Verification:** all 144 checkpoint validation aggregates match the episode
index; 138 selected/final probe episodes replay exactly; 23 selected/final
actors reload and reproduce nominal returns exactly. All metrics are finite
and every calibration/training decision is accounted for. The saved 10k
normalization matches PPO on all twelve corresponding pilot seeds exactly.
The final audit adds 9,898 actor-evaluation decisions and 62,569 replayed
diagnostic decisions; those are separate from pilot training/evaluation cost.

Regenerate the checked evidence and figure (from the repository root):

```bash
env -u PYTHONPATH uv run --frozen --no-sync python experiments/ese3990/audit_runs.py \
  runs/ese3990/sac/pilots/S1_base_s*/ --out docs/ese3990/data/M2_sac_pilots.json
MPLCONFIGDIR=/tmp/mpl env -u PYTHONPATH uv run --frozen --no-sync \
  python experiments/ese3990/plots.py runs/ese3990/sac/pilots/S1_base_s*/ \
  --table --out docs/ese3990/figs/M2_sac_R_pilots.png
```

Raw compact evidence: [M2_sac_pilots.json](../ese3990/data/M2_sac_pilots.json).
The [M2 report](../ese3990/M2_report.md) records the configuration freeze,
main queue and correctness checks.
