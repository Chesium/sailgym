# M6 · Simulator-verified mirrored self-imitation

Three matched PPO seeds ran for 400,000 online decisions each in 4.3 minutes wall time. All 1,902 new episode records replayed exactly. Both arms used broad starts at rest, 1–3 m below the gate, the same PPO optimizer, and self-imitation weight 0.1. Each mirrored arm used only its own online successes to generate extra attempts. [Frozen protocol](../../runs/ese3990/M6/mirror/protocol.json) · [Machine-readable results](M6_mirror_results.json)

## Fixed-final finishes by starting side

Each cell gives finishes out of 40. Control is the ordinary self-imitation checkpoint on the same seed and starts. The previous panel is where the east–west discrepancy was identified. The new panel was frozen for this experiment. The paired panel contains 40 reflected start pairs.

| Panel | Seed | West control | West mirror | East control | East mirror |
|---|---:|---:|---:|---:|---:|
| paired | 0 | 0 | 1 | 0 | 0 |
| paired | 1 | 7 | 8 | 6 | 8 |
| paired | 2 | 0 | 11 | 9 | 12 |
| previous | 0 | 0 | 0 | 0 | 0 |
| previous | 1 | 10 | 6 | 5 | 9 |
| previous | 2 | 0 | 3 | 11 | 9 |
| new | 0 | 0 | 0 | 0 | 0 |
| new | 1 | 7 | 11 | 1 | 3 |
| new | 2 | 1 | 13 | 4 | 3 |

## Extra simulation and promotion

| Seed | Online finishes | Reflected finishes / attempts | Extra decisions | Extra rows admitted | Pairs finished on both sides, control → mirror | Mirror promotion finishes per 20 | 40 m finishes, control → mirror / 20 |
|---:|---:|---:|---:|---:|---:|---|---:|
| 0 | 2 | 2/2 | 570 | 570 | 0 → 0 | 1, 0, 0, 1 | 0 → 0 |
| 1 | 10 | 10/10 | 2470 | 2470 | 4 → 8 | 6, 0, 7, 5, 4 | 0 → 0 |
| 2 | 53 | 53/53 | 16119 | 16119 | 0 → 11 | 0, 0, 1, 3, 2, 5, 3, 3 | 0 → 0 |

0/3 seeds passed two successive fresh probes with at least 6/20 finishes each above the fixed controls. The 3–7 m paired transfer gate stayed closed.

The numerical results identify the individual starts gained, retained and lost on each side. Reproduce this report after training with `env -u PYTHONPATH .venv/bin/python experiments/ese3990/m6_mirror_report.py`. [Training and replay code](../../experiments/ese3990/m6_mirror.py) · [Run records](../../runs/ese3990/M6/mirror)
