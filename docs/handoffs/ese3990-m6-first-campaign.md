# M6 first reverse-start campaign

The user authorized launch on 2026-11-19. The first simulator job started at 07:32:58 Singapore time. Training closes by 09:22:58 and the campaign budget ends at 10:02:58. The [campaign plan](../ese3990/ESE3990_SailGym_Project_Plan.md#first-campaign--150-minutes-on-the-existing-cpu-machine) sets the pilot gate and the 150-minute ceiling.

The completed local user service was `sailgym-m6-first.service`. Inspect it with `systemctl --user status sailgym-m6-first --no-pager` or consult the [run directory](../../runs/ese3990/M6/first). The [supervisor](../../experiments/ese3990/m6_campaign.py) calibrates the reset geometry, starts both seed-100 pilots, applies the pilot gate, seals checkpoint identities and audits saved episodes. It uses the existing CPU environment. Successful early phases can release time to the next phase.

To reproduce in a clean checkout with an empty output directory, run `env -u PYTHONPATH OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 XLA_FLAGS=--xla_cpu_multi_thread_eigen=false .venv/bin/python experiments/ese3990/m6_campaign.py`. The supervisor refuses to overwrite an existing campaign. Per-run policy snapshots support evaluation. They omit optimizer and replay state. An interrupted learner requires a fresh run from its seed to reproduce training.

The first-box control recorded 0/100 finishes with neutral controls and 4/100 with pink noise. Both focused M6 tests passed. The initial live quota check reported 76% remaining in the five-hour window and 82% in the weekly window.

The service finished at 07:41:30 Singapore time. Both pilots completed 400k decisions and stayed at stage 0. The main-run decision was false, the two final validation panels completed, and all 868 episode records replayed exactly. No campaign jobs remain. [Result and analysis](../ese3990/M6_first_report.md) · [Numerical results](../ese3990/M6_first_results.json)
