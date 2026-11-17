| Controller / role | Set | Finishes | Per seed | Outcomes | Successful mean (s) |
|---|---|---|---|---|---|
| PPO / selected | nominal | 0/3 | 0/1, 0/1, 0/1 | truncated 3 | — |
| PPO / selected | test | 0/150 | 0/50, 0/50, 0/50 | mark_missed 2, out_of_bounds 4, truncated 144 | — |
| PPO / selected | shift | 0/60 | 0/20, 0/20, 0/20 | capsized 1, out_of_bounds 1, truncated 58 | — |
| PPO / final | nominal | 0/3 | 0/1, 0/1, 0/1 | truncated 3 | — |
| PPO / final | test | 0/150 | 0/50, 0/50, 0/50 | mark_missed 2, out_of_bounds 10, truncated 138 | — |
| PPO / final | shift | 0/60 | 0/20, 0/20, 0/20 | out_of_bounds 3, truncated 57 | — |
| SAC / selected | nominal | 0/3 | 0/1, 0/1, 0/1 | truncated 3 | — |
| SAC / selected | test | 0/150 | 0/50, 0/50, 0/50 | truncated 150 | — |
| SAC / selected | shift | 0/60 | 0/20, 0/20, 0/20 | out_of_bounds 1, truncated 59 | — |
| SAC / final | nominal | 0/3 | 0/1, 0/1, 0/1 | truncated 3 | — |
| SAC / final | test | 0/150 | 0/50, 0/50, 0/50 | out_of_bounds 3, truncated 147 | — |
| SAC / final | shift | 0/60 | 0/20, 0/20, 0/20 | truncated 60 | — |
| SAC + demonstrations (expert data) / selected | nominal | 0/3 | 0/1, 0/1, 0/1 | mark_missed 1, truncated 2 | — |
| SAC + demonstrations (expert data) / selected | test | 5/150 | 4/50, 1/50, 0/50 | finished 5, mark_missed 45, out_of_bounds 2, truncated 98 | 68.55 |
| SAC + demonstrations (expert data) / selected | shift | 0/60 | 0/20, 0/20, 0/20 | out_of_bounds 2, truncated 58 | — |
| SAC + demonstrations (expert data) / final | nominal | 0/3 | 0/1, 0/1, 0/1 | mark_missed 1, truncated 2 | — |
| SAC + demonstrations (expert data) / final | test | 0/150 | 0/50, 0/50, 0/50 | mark_missed 23, out_of_bounds 4, truncated 123 | — |
| SAC + demonstrations (expert data) / final | shift | 0/60 | 0/20, 0/20, 0/20 | mark_missed 1, truncated 59 | — |
| PPO + BC (expert data) / selected | nominal | 0/3 | 0/1, 0/1, 0/1 | truncated 3 | — |
| PPO + BC (expert data) / selected | test | 0/150 | 0/50, 0/50, 0/50 | truncated 150 | — |
| PPO + BC (expert data) / selected | shift | 0/60 | 0/20, 0/20, 0/20 | truncated 60 | — |
| PPO + BC (expert data) / final | nominal | 0/3 | 0/1, 0/1, 0/1 | truncated 3 | — |
| PPO + BC (expert data) / final | test | 0/150 | 0/50, 0/50, 0/50 | out_of_bounds 8, truncated 142 | — |
| PPO + BC (expert data) / final | shift | 0/60 | 0/20, 0/20, 0/20 | mark_missed 5, out_of_bounds 2, truncated 53 | — |
| BC only (expert data) / bc_only | nominal | 0/3 | 0/1, 0/1, 0/1 | out_of_bounds 1, truncated 2 | — |
| BC only (expert data) / bc_only | test | 0/150 | 0/50, 0/50, 0/50 | mark_missed 1, out_of_bounds 25, truncated 124 | — |
| BC only (expert data) / bc_only | shift | 0/60 | 0/20, 0/20, 0/20 | mark_missed 6, out_of_bounds 1, truncated 53 | — |
| True-model MPC / selected | nominal | 3/3 | 1/1, 1/1, 1/1 | finished 3 | 48.67 |
| True-model MPC / selected | test | 150/150 | 50/50, 50/50, 50/50 | finished 150 | 48.00 |
| True-model MPC / selected | shift | 60/60 | 20/20, 20/20, 20/20 | finished 60 | 51.40 |
| RuleSailor / reference | nominal | 1/1 | 1/1 | finished 1 | 62.58 |
| RuleSailor / reference | test | 38/50 | 38/50 | finished 38, mark_missed 8, truncated 4 | 71.53 |
| RuleSailor / reference | shift | 2/20 | 2/20 | finished 2, mark_missed 8, truncated 10 | 105.59 |
