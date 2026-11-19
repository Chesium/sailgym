# M6 width and heading follow-up

The campaign started at 11:21:43 Singapore time on 2026-11-19 under the local user service `sailgym-m6-width.service`. The [project plan](../ese3990/ESE3990_SailGym_Project_Plan.md#m6-follow-up--learn-the-first-approach-before-moving-downwind) and [protocol](../../runs/ese3990/M6/width/protocol.json) specify the six PPO runs, exact panels and conditional branches. Initial live allowance was 97% over five hours and 78% weekly.

Read [status](../../runs/ese3990/M6/width/status.json), each worker's log in the same directory, and `journalctl --user -u sailgym-m6-width`. The [runner](../../experiments/ese3990/m6_width.py) uses the original M6 trainer in a separate process for each seed. To reproduce in a checkout with the first campaign's checkpoints available and a fresh output directory, run `env -u PYTHONPATH OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 XLA_FLAGS=--xla_cpu_multi_thread_eigen=false .venv/bin/python experiments/ese3990/m6_width.py campaign`. The runner refuses to overwrite existing outputs. Once the status is complete, generate the report with `env -u PYTHONPATH .venv/bin/python experiments/ese3990/m6_width_report.py`. The reporter checks audit episode IDs against the stored index.

The common-panel retention check completed before fresh training. Finishes on the fixed 80 starts were 16, 7, 0, 0, 0, 0 in checkpoint order.

Completed in 12 min 20 s. All nine 400k-decision PPO runs completed and qualified 0/3 seeds in each geometry. All 5,219 episode records replayed exactly. The conditional transfer and SAC branches stayed closed. [Report](../ese3990/M6_width_report.md) · [Numerical results](../ese3990/M6_width_results.json)
