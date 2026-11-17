| Controller / role | Set | Episodes with a tack | Mean tacks | Mean failed tacks | Mean irons (s) | Mark missed | Out of bounds | Timeouts | Timeout final distance, median (m) |
|---|---|---|---|---|---|---|---|---|---|
| PPO / selected | nominal | 0/3 | 0.00 | 0.67 | 10.4 | 0 | 0 | 3 | 66.7 |
| PPO / selected | test | 40/150 | 0.32 | 0.12 | 5.5 | 2 | 4 | 144 | 66.3 |
| PPO / selected | shift | 9/60 | 0.15 | 0.05 | 4.5 | 0 | 1 | 58 | 70.9 |
| PPO / final | nominal | 0/3 | 0.00 | 0.33 | 7.2 | 0 | 0 | 3 | 72.3 |
| PPO / final | test | 10/150 | 0.08 | 0.07 | 3.8 | 2 | 10 | 138 | 74.1 |
| PPO / final | shift | 2/60 | 0.03 | 0.02 | 4.0 | 0 | 3 | 57 | 72.0 |
| SAC / selected | nominal | 0/3 | 0.00 | 0.00 | 0.0 | 0 | 0 | 3 | 59.7 |
| SAC / selected | test | 0/150 | 0.00 | 0.00 | 0.5 | 0 | 0 | 150 | 57.0 |
| SAC / selected | shift | 1/60 | 0.02 | 0.00 | 0.2 | 0 | 1 | 59 | 66.5 |
| SAC / final | nominal | 0/3 | 0.00 | 0.00 | 0.0 | 0 | 0 | 3 | 59.7 |
| SAC / final | test | 0/150 | 0.00 | 0.00 | 1.4 | 0 | 3 | 147 | 56.9 |
| SAC / final | shift | 0/60 | 0.00 | 0.00 | 0.2 | 0 | 0 | 60 | 70.2 |
| SAC + demonstrations (expert data) / selected | nominal | 3/3 | 1.33 | 0.00 | 1.6 | 1 | 0 | 2 | 29.8 |
| SAC + demonstrations (expert data) / selected | test | 81/150 | 0.67 | 0.04 | 3.7 | 45 | 2 | 98 | 62.6 |
| SAC + demonstrations (expert data) / selected | shift | 3/60 | 0.05 | 0.00 | 0.7 | 0 | 2 | 58 | 64.0 |
| SAC + demonstrations (expert data) / final | nominal | 0/3 | 0.00 | 0.00 | 0.0 | 1 | 0 | 2 | 80.7 |
| SAC + demonstrations (expert data) / final | test | 17/150 | 0.11 | 0.02 | 5.4 | 23 | 4 | 123 | 64.9 |
| SAC + demonstrations (expert data) / final | shift | 1/60 | 0.02 | 0.13 | 6.7 | 1 | 0 | 59 | 70.7 |
| PPO + BC (expert data) / selected | nominal | 0/3 | 0.00 | 0.00 | 3.9 | 0 | 0 | 3 | 66.6 |
| PPO + BC (expert data) / selected | test | 20/150 | 0.13 | 0.00 | 3.7 | 0 | 0 | 150 | 69.4 |
| PPO + BC (expert data) / selected | shift | 2/60 | 0.05 | 0.00 | 0.8 | 0 | 0 | 60 | 86.6 |
| PPO + BC (expert data) / final | nominal | 0/3 | 0.00 | 0.00 | 4.8 | 0 | 0 | 3 | 116.2 |
| PPO + BC (expert data) / final | test | 14/150 | 0.09 | 0.00 | 4.2 | 0 | 8 | 142 | 113.5 |
| PPO + BC (expert data) / final | shift | 2/60 | 0.05 | 0.00 | 0.5 | 5 | 2 | 53 | 107.4 |
| BC only (expert data) / bc_only | nominal | 0/3 | 0.00 | 0.00 | 0.0 | 0 | 1 | 2 | 106.9 |
| BC only (expert data) / bc_only | test | 0/150 | 0.00 | 0.00 | 0.0 | 1 | 25 | 124 | 123.5 |
| BC only (expert data) / bc_only | shift | 0/60 | 0.00 | 0.00 | 0.0 | 6 | 1 | 53 | 109.2 |
| True-model MPC / selected | nominal | 3/3 | 2.00 | 0.00 | 0.0 | 0 | 0 | 0 | — |
| True-model MPC / selected | test | 130/150 | 1.54 | 0.13 | 0.0 | 0 | 0 | 0 | — |
| True-model MPC / selected | shift | 59/60 | 1.60 | 0.02 | 0.0 | 0 | 0 | 0 | — |
| RuleSailor / reference | nominal | 1/1 | 2.00 | 0.00 | 0.0 | 0 | 0 | 0 | — |
| RuleSailor / reference | test | 49/50 | 1.96 | 0.22 | 5.9 | 8 | 0 | 4 | 37.0 |
| RuleSailor / reference | shift | 10/20 | 1.00 | 0.95 | 40.9 | 8 | 0 | 10 | 93.5 |
