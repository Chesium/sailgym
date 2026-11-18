# Does ten times the training budget help blocked SAC finish the upwind course?

**Training and checkpoint validation are complete.** Five seeds trained from scratch for 40M decisions each. None finished the 40 m upwind course in any scheduled validation episode. The 40M configuration therefore missed the predeclared threshold of at least three seeds with a validation finish. [Frozen protocol](../../experiments/ese3990/configs/M5_sac_budget_freeze.json)

| Observed outcome | 40M blocked SAC |
|---|---:|
| Seeds with a selected-checkpoint validation finish | 0/5 |
| Finishes at the five fixed 40M checkpoints | 0/100 |
| Finishes across all ten validation checkpoints per seed | 0/1,000 |
| Completed training episodes that finished at the gate | 0/55,611 |
| Parallel training and checkpoint validation time | 2 h 31 min |

The same 20 validation conditions were repeated at each checkpoint, so the 1,000 evaluations are repeated observations across training. The 4M comparison used the same five seeds, standard start, 0.5 s action holds and reward. At 40M, each learner made approximately four million gradient updates. Four seeds kept their critic estimates inside the task-return reference band of [−22.3, +15.7] throughout training. Seed 4 reached −23.11. At 4M all five were inside the band. [Stage-1 comparison](M5_report.md) · [Per-seed records](../../runs/ese3990/M5_sac_budget/extended)

The extra budget gave SAC more opportunities to experience a finish and learn from it. Across these five seeds, the training records contain no such finish. This result applies to blocked SAC with the standard start and the frozen task reward. Whether more training changed tacking or brought the boat closer to the gate is measured separately from finishing.

## Held-out evaluation and replay

At the final 40M checkpoint, 0/100 validation episodes contained a settled tack. The boats averaged 4.0 m of maximum upwind progress toward the 40 m gate. Selected checkpoints had six opportunities at 4M and ten at 40M. The final-checkpoint rows compare fixed budgets. Gate distance is the mean closest approach to the gate center.

| Budget | Checkpoint | Validation finishes | Validation tacks | Max upwind | Gate distance | Test finishes | Shift finishes |
|---|---|---:|---:|---:|---:|---:|---:|
| 4M | selected | 0/100 | 0/100 | 2.6 | 39.1 | 0/250 | 0/100 |
| 4M | final | 0/100 | 0/100 | 1.3 | 39.7 | 0/250 | 0/100 |
| 40M | selected | 0/100 | 0/100 | 3.9 | 37.2 | 0/250 | 0/100 |
| 40M | final | 0/100 | 0/100 | 4.0 | 37.1 | 0/250 | 0/100 |

All 56,681 stored training-run episodes and 568 new held-out episodes replayed exactly. Shared checkpoint comparisons, training finishes, Q ranges, entropy temperatures and rudder travel are in the [paired results](M5_sac_budget_results.json).
