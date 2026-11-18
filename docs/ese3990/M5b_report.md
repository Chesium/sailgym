# M5b exploratory upwind study

M5 closes. None of the four arms met the validation finish threshold at 4M decisions.

Each arm had five seeds. The frozen threshold required at least three seeds with at least one finish in 20 selected validation episodes. Held-out columns use those selected checkpoints.

| Arm | Qualifying seeds | Validation finishes | Episodes with a tack | Mean max y (m) | Test finishes | Shift finishes | Training finishes |
|---|---:|---:|---:|---:|---:|---:|---:|
| SAC baseline | 0/5 | 0/100 | 1/100 | 3.7 | 0/250 | 0/100 | 0 |
| PPO baseline | 0/5 | 0/100 | 20/100 | 10.1 | 0/250 | 0/100 | 0 |
| SAC-E | 0/5 | 0/100 | 0/100 | 0.4 | 0/250 | 0/100 | 0 |
| PPO-8 | 0/5 | 0/100 | 0/100 | 0.8 | 0/250 | 0/100 | 0 |
| SAC-P | 0/5 | 0/100 | 0/100 | 1.0 | 0/250 | 0/100 | 0 |
| PPO-sigma | 0/5 | 0/100 | 1/100 | 1.4 | 0/250 | 0/100 | 0 |

SAC-E kept Q inside the reference band in 0/5 seeds. The audit reproduced 24,913 training-run and 2,627 held-out episodes. The paired seed data include selected and final checkpoints, critic scale, entropy temperature and rudder travel. [Results](M5b_results.json) · [Claims](M5b_claims.json)
