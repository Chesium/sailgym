# M6 follow-up · Lateral offset and initial heading

The campaign took 12.3 minutes and exactly replayed 5,219 episode records. Each fresh fit used 400,000 decisions, including 10,000 for calibration. A seed qualified on two successive fresh probes at 6/20 or higher, each above both controls. [Frozen plan](ESE3990_SailGym_Project_Plan.md#m6-follow-up--learn-the-first-approach-before-moving-downwind) · [Complete numerical results](M6_width_results.json)

## Fixed-panel retention on the first seed-100 PPO campaign

| Decisions | Inside gate span / 40 | Outside / 40 | Gained since previous | Lost since previous |
|---:|---:|---:|---:|---:|
| 77,991 | 2 | 14 | – | – |
| 143,548 | 1 | 6 | 0 | 9 |
| 219,759 | 0 | 0 | 0 | 7 |
| 309,759 | 0 | 0 | 0 | 0 |
| 399,759 | 0 | 0 | 0 | 0 |
| 400,000 | 0 | 0 | 0 | 0 |

The same 80 condition identities appear in all six rows. The numerical results list each gained and lost finish by condition ID.

## Fixed-final panels and fresh promotion probes

| Training geometry | Learner / seed | Promotion finishes per 20 | First qualifying decision | Narrow / 80 | Broad / 80 | Close-hauled / 80 | Original 40 m / 20 |
|---|---|---|---:|---:|---:|---:|---:|
| broad | ppo_s0 | 1, 0, 0, 0 | – | 0 | 0 | 0 | 0 |
| broad | ppo_s1 | 6, 0, 2, 0 | – | 2 | 2 | 0 | 0 |
| broad | ppo_s2 | 0, 0, 2, 1, 2 | – | 4 | 7 | 0 | 0 |
| narrow | ppo_s0 | 0, 0, 0, 0 | – | 0 | 0 | 0 | 0 |
| narrow | ppo_s1 | 4, 0, 0, 0 | – | 0 | 0 | 0 | 0 |
| narrow | ppo_s2 | 0, 0, 1, 0 | – | 1 | 2 | 0 | 0 |
| heading | ppo_s0 | 0, 4, 0, 0 | – | 0 | 0 | 0 | 0 |
| heading | ppo_s1 | 0, 0, 0, 0 | – | 0 | 2 | 0 | 0 |
| heading | ppo_s2 | 0, 0, 0, 0 | – | 1 | 3 | 0 | 0 |

Controls used 100 starts on each geometry. Neutral and pink-noise finishes respectively were broad 0/100 and 7/100, narrow 0/100 and 4/100, heading 0/100 and 8/100. All fixed-final panels use deterministic policies. The random-heading narrow and broad panels share wind, heading, depth and normalized lateral offset. The close-hauled panel shares positions and winds with the narrow panel.

## Campaign decision

- Broad training qualified 0/3 seeds. Fixed-final policies finished 9/240 broad and 6/240 narrow random-heading evaluations.
- Narrow training qualified 0/3 seeds. Fixed-final policies finished 2/240 broad and 1/240 narrow random-heading evaluations.
- Heading training qualified 0/3 seeds. Fixed-final policies finished 5/240 broad and 1/240 narrow random-heading evaluations.

Both changes missed the two-seed threshold. The campaign closed before the 3–7 m transfer or SAC pilot. The common-panel comparison shows which successful approaches the first seed lost as training continued. The mechanism remains open.

[Run protocol and panels](../../runs/ese3990/M6/width/protocol.json) · [Per-episode audits and checkpoints](../../runs/ese3990/M6/width) · [Campaign runner](../../experiments/ese3990/m6_width.py)
