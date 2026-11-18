# Five-seed SAC budget study on the upwind gate

Training completed at 6:03 p.m. Eastern on November 17. All five seeds reached 40M decisions. The [study writeup](../ese3990/M5_sac_budget_report.md) records validation and will receive the audited 4M and 40M comparison at completion.

Five local CPU runs started together on November 17, 2026 at 3:31:58 p.m. Eastern (20:31:58 UTC). The study gets an estimated three hours of training wall time. Checkpoint validation is included. Exact replay and held-out scoring follow. The user authorized planning and starting the study. A user-level systemd service owns the workers and the follow-on evaluation independently of this chat.

## Protocol

The [project plan](../ese3990/ESE3990_SailGym_Project_Plan.md#m5-sac-budget-extension--five-seeds-about-three-hours-of-cpu-training) and [training freeze](../../experiments/ese3990/configs/M5_sac_budget_freeze.json) specify a single exploratory arm. SHA-256 of the freeze is `b768ee0043cd6133d0e81758b8e92f844a6f53183156808572c2df982bd50ad3`. The original frozen M5 SAC B configuration on U and its seeds 0–4 supply the paired baseline. The two config differences are raw decision budget (4M to 40M) and checkpoint schedule (four additional targets). Starts, rewards, action holds, training code and CPU backend are the same. Each training run counts its 10k calibration toward 40M decisions.

Predeclared success requires at least three of five validation-selected policies to finish in at least one of 20 episodes each. Report held-out finish counts on test and shift, the fixed final checkpoints, tacking and closest approach to the gate. There are ten checkpoint opportunities in the extended runs and six in the original. Stage-1 evidence and M5b use their own freezes. The study trains each seed from initialization, because the prior policy snapshots do not preserve replay and optimizer state.

## Running job and outputs

- Supervisor unit `sailgym-m5-sac-budget.service`, initial PID 4079414, systemd invocation `2c82f46af62746b7810f9c48ed72005f`. Sources are [supervisor](../../experiments/ese3990/m5_sac_budget.py) and [evaluator](../../experiments/ese3990/m5_sac_budget_evaluate.py). Branch `ese3990` at launch was `fd82b0b5d7c86fe6d5fbec5ccf7c9d92fa70e8be`. Earlier uncommitted M5 work was present.
- [Phase and failure status](../../runs/ese3990/M5_sac_budget/status.json) records training, audit and evaluation, completion or failure. [Per-seed directory](../../runs/ese3990/M5_sac_budget/extended) contains worker logs, incrementing `metrics.jsonl`, validation curves and checkpoints. Use `systemctl --user status sailgym-m5-sac-budget --no-pager` and `journalctl --user -u sailgym-m5-sac-budget -n 30 --no-pager` for live service status.
- After training, the evaluator checks parameters and validation at matching early checkpoints against stage 1. A full block in the 40M run can cross 4M by up to nine raw decisions. The evaluator then audits stored training-run episodes, freezes selected/final checkpoint hashes before opening the held-out sets and replays every new held-out episode. It writes `heldout/freeze.json`, `results.json` and `report.md` under `runs/ese3990/M5_sac_budget/`.
- On failure, check the affected seed log and service journal. The supervisor retains incomplete runs. A saved policy alone cannot resume training with the same optimizer and replay state. The evaluator can reuse verified run audits and completed held-out episodes. To resume after diagnosis, use the same Python environment with `PYTHONPATH` unset and `XLA_FLAGS=--xla_cpu_multi_thread_eigen=false`. Once all five summaries exist, the standalone evaluation command is `env -u PYTHONPATH XLA_FLAGS=--xla_cpu_multi_thread_eigen=false .venv/bin/python experiments/ese3990/m5_sac_budget_evaluate.py --jobs 5`. The evaluator's lock prevents duplicate writers.

## Launch verification

The original M5 freeze, native build identity, new freeze and baseline hashes verified. The 27 existing M5, follow-up and reporting tests and two focused budget-evaluation tests passed. The five calibration-only parameter files and validation rows matched their corresponding 4M files byte-for-byte or field-for-field. At 3:33 p.m. Eastern all five seeds had also passed the 400k target, with byte-identical checkpoint parameters and equal validation rows at that target. Extended-budget results are pending at launch. No GPU or physics-port work is part of this study.

The usage guard read 73% five-hour and 53% weekly headroom at startup. Computational work under the service continues locally without model turns. Review of the launcher was assigned to `review_study` and the evaluator to `eval`. The primary agent owns the training freeze, protocol and service.
