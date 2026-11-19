# M6 · Opening-policy handoff and PPO self-imitation

The campaign completed in 6.6 minutes. Exact replay covered 2,468 new episodes. The policy remained frozen during every evaluation. [Protocol](../../runs/ese3990/M6/imitation/protocol.json) · [Results by seed and condition](M6_imitation_results.json)

## Two-second handoff on seed 100

| Opening → remainder | Finishes / 80 | Retained early wins / 16 | Other finishes / 64 |
|---|---:|---:|---:|
| early → early | 16 | 16 | 0 |
| early → late | 0 | 0 | 0 |
| late → early | 10 | 10 | 0 |
| late → late | 0 | 0 | 0 |

Early and late refer to 77,991 and 400,000 decisions. The controls matched the archived actions at every step. The first 40 actions of each hybrid matched its opening control on every condition.

## Broad starts, 400k raw decisions per seed

| Seed | Policy | Common anchor retained | First probe retained | Final fixed / 80 | Final fresh / 80 | Fresh promotion wins per 20 | 40 m validation / 20 |
|---:|---|---:|---:|---:|---:|---|---:|
| 0 | baseline | 0/1 | 0/9 | 0 | 0 | 1, 0, 0, 0 | 0 |
| 0 | imitation | 0/1 | 0/9 | 0 | 0 | 1, 0, 0, 0 | 0 |
| 1 | baseline | 2/9 | 2/8 | 2 | 0 | 6, 0, 2, 0 | 0 |
| 1 | imitation | 5/9 | 6/8 | 13 | 15 | 6, 0, 1, 3 | 0 |
| 2 | baseline | 4/7 | 0/0 | 7 | 3 | 0, 0, 2, 1, 2 | 0 |
| 2 | imitation | 7/7 | 0/0 | 9 | 11 | 0, 0, 2, 4, 1 | 0 |

Each common anchor was saved immediately before the first update with self-imitation. Actions through that rollout matched ordinary PPO exactly. A denominator of zero means retention could not be measured for that snapshot. The first-probe comparison uses each arm's own earliest promotion snapshot. All retention counts use identical start conditions.

| Seed | First training finish at decision | First affected update after decision | Successful episodes | Replay transitions | PPO minibatches with auxiliary loss |
|---:|---:|---:|---:|---:|---:|
| 0 | 161417 | 161552 | 1 | 241 | 3756 |
| 1 | 225305 | 227088 | 4 | 3207 | 2732 |
| 2 | 254556 | 255760 | 14 | 5741 | 2284 |

The buffer ledger matched every completed online training success. Each PPO minibatch after activation added an actor loss with coefficient 0.1. The detached weight was the positive part of discounted return-to-go minus current value. Sampling used a separate random stream. The ordinary PPO controls were the previously completed matched broad-start runs.

0/3 seeds qualified on two successive fresh promotion probes. Pooled common-anchor finishes retained at 400k were 12 for self-imitation and 6 for ordinary PPO. The 3–7 m transfer gate stayed closed.

## Reading across seeds

- Baseline finished 3/240 new near-gate evaluations and 0/60 original-start validations.
- Imitation finished 26/240 new near-gate evaluations and 0/60 original-start validations.

Counts pool three matched seeds evaluated on the same conditions. The per-seed rows show the variation across policies.

[Campaign runner](../../experiments/ese3990/m6_imitation.py) · [Episode stores and checkpoints](../../runs/ese3990/M6/imitation)
