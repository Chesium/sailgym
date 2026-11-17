# ESE3990 experiments

Learning versus planning on SailGym: PPO, SAC and true-model CEM-MPC, with
RuleSailor as an engineering reference. The plan is
[`docs/ese3990/ESE3990_SailGym_Project_Plan.md`](../../docs/ese3990/ESE3990_SailGym_Project_Plan.md);
the M0 evidence is in [`docs/ese3990/M0_report.md`](../../docs/ese3990/M0_report.md).
SAC checks, pilot results, resource deviations and the frozen 1M main budget
are in the [M2 report](../../docs/ese3990/M2_report.md). Exact learner settings
and seeds are saved in [`configs/M2_freeze.json`](configs/M2_freeze.json).
MPC implementation, validation sweep and online cost are documented in the
[M3 report](../../docs/ese3990/M3_report.md); its selected default is saved in
[`configs/M3_freeze.json`](configs/M3_freeze.json).
The [M4 report](../../docs/ese3990/M4_report.md) contains the frozen test/shift
comparison, exact replay audit, failure analysis and claim-to-command index.
Its figures and video specs are generated from the saved episode stores.

Physics, gate passage, capsize, bounds and time limits are Rust decisions
(`sailgym_core`). Nothing in this directory models the boat.

## Layout

| File | Role |
|---|---|
| `configs/tasks.json` | Tasks R and U, bounds, cadence, action restriction, reward constants |
| `configs/tasks_v2.json` | M4U contract 2: U's γ = 0.99975. `tasks.json` stays contract 1 (its hash is replay identity); U v2 episodes record `contract_version` and replay under it |
| `configs/M4U_*.json` | M4U training, MPC and diagnostics-threshold freezes |
| `configs/conditions/*.json` | Generated condition manifests (validation 20, test 50, shift 20; training sampler spec) |
| `make_conditions.py` | Deterministic manifest generator; `--check` verifies the committed files |
| `sailing.py` | Shared contract: `Task`, `Condition`, `ActionAdapter`, `features`, `TransitionReward`, `masks`, `TaskEnv`, `run_metadata` |
| `baselines.py` | `SmoothRandom` and `RuleSailorController` (bound Rust RuleSailor, release forced off) |
| `evaluate.py` | One evaluator for every controller (`--controller ppo --checkpoint …` too); writes an outcome report |
| `ppo.py` | PPO: networks, tanh-Gaussian likelihood, GAE, clipped loss, checkpoints, `PPOPolicy`. Comments cite the slides by PDF page |
| `sac.py` | SAC: replay buffer, twin Q + Polyak targets, state-dependent tanh-Gaussian actor, learned temperature, `SACPolicy`. Reuses `ppo.py`'s MLP, likelihood and checkpoint format. Comments cite the slides by PDF page |
| `mpc.py` | CEM-MPC: blocked Gaussian proposals, elite mean/std, shifted warm starts, best sampled plan; comments link the mechanism to advanced PDF pp. 120, 178–179 and 199–201 |
| `mpc_study.py` | Resumable five-setting validation sweep, actual simulator work and latency, validation selection and planned-path probes |
| `mpc_figures.py` | Cost/success and latency figure; predicted plan versus exactly replayed execution |
| `compare.py` | M4 freeze and resumable nominal/test/shift evaluation of all predeclared checkpoints and frozen references |
| `comparison_report.py` | Verify frozen inputs, outcome/cost summaries, test learning curves, representative selections and JSON video specs |
| `demos.py` | M4U demonstrations: 50 RuleSailor U episodes on training conditions, store hash, exact-replay transition loader with returns-to-go |
| `m4u_compare.py` | M4U held-out freeze and parallel, resumable nominal/test/shift evaluation of every arm's predeclared checkpoints, the selected U MPC and RuleSailor |
| `m4u_report.py` | Verify M4U frozen inputs; outcome, diagnostics and cost tables; learning curves; video specs |
| `u_diagnostics.py` | U behaviour from exact replay: tacks, failed tacks, irons, sternway, failure flags, timeout distance |
| `m4x.py` | M4X (post-hoc, exploratory): V check of X2's value on held-out RuleSailor episodes, the freeze (`configs/M4X_freeze.json`, amended by `M4X_freeze_amendment1.json`), and the X2 / held-out evaluation driver (`ValueTail` = PPO + BC's initial critic) |
| `m4x_report.py` | M4X critic scale (all SAC runs, raw and entropy-adjusted), X1/X1b validation curves, X2 outcomes/latency/predicted-vs-executed return, held-out rows |
| `probe.py` | M4P (post-hoc, exploratory, no learning): random rate noise of three colours × four holds × three scales on 100 paired U training conditions, plus `SmoothRandom(hold=10)`. Freeze, runner, and a report that replays every episode exactly and counts tacks with `u_diagnostics` |
| `mode_check.py` | Is RuleSailor's manoeuvre mode predictable from the 17 learner features? |
| `replay_audit.py` | Replay every episode of one or more stores exactly, in parallel |
| `viz/compose.py` | Batch static/GIF/MP4 overlays from exact stored-episode selectors; rejects mixed conditions |
| `train.py` | Learner entry point (`--learner ppo|sac`): calibration and normalization (identical for both), collection (every episode stored), updates, checkpoints and probe/validation evaluation |
| `plots.py` | Learning curve from runs' `curve.jsonl` |
| `audit_runs.py` | Verify saved costs, finite metrics, validation aggregates, exact selected/final probe replay and nominal actor reload; writes a compact JSON evidence file |
| `viz/episode_store.py` | Episode logs (actions + condition + seed + build identity + checksum + 5 Hz pose) in compressed shards with an `index.jsonl` (plan §10) |
| `viz/replay.py` | Exact re-simulation of a stored episode, verified against the log; pose-only fallback |
| `viz/render.py` | Static track figure, time series, and a synced multi-episode gif/mp4 overlay |
| `viz/show.py` | Command-line entry: select episodes from a store, replay them and render |
| `tests/` | Contract, branch, baseline and viz checks |

M4's generated results and media commands are recorded in its report and specs.
M4U's are in [`docs/ese3990/M4U_report.md`](../../docs/ese3990/M4U_report.md). PPO + BC runs add `bc.jsonl` (one line per BC epoch).

## Commands

Run from the repository root. Unset `PYTHONPATH` if a ROS (or other) install
puts its packages on it — `scripts/py-test.sh` does the same. Use
`uv run --frozen --no-sync`: a plain `uv run` re-syncs from the lock and
replaces the freshly built `sailgym_core` with a stale cached wheel.
Training requires a new output directory; existing runs are never overwritten.

```bash
# build the extension after any Rust change
uv run --frozen --no-sync maturin develop --uv --release --manifest-path crates/sailgym-py/Cargo.toml

# experiment tests (not part of the default `pytest python` discovery)
env -u PYTHONPATH uv run --frozen --no-sync pytest experiments/ese3990/tests

# Rust branch-vs-replay proofs
cargo test -p sailgym-env --test fork

# verify the committed manifests
uv run --frozen --no-sync python experiments/ese3990/make_conditions.py --check

# an outcome report
uv run --frozen --no-sync python experiments/ese3990/evaluate.py --task R --controller rule_sailor \
    --manifest validation --out runs/ese3990/eval/R_rule_validation.json

# M3: five settings × 20 validation conditions × search seeds [0,1,2]
# Candidate branches run on the existing Rayon pool. Allow roughly an hour
# on the measured CPU; use a fresh output directory for a new measurement.
env -u PYTHONPATH RAYON_NUM_THREADS=12 uv run --frozen --no-sync python \
    experiments/ese3990/mpc_study.py --out runs/ese3990/mpc/M3
# Select the default, then record/replay nominal + five validation probes.
env -u PYTHONPATH RAYON_NUM_THREADS=12 uv run --frozen --no-sync python \
    experiments/ese3990/mpc_study.py --out runs/ese3990/mpc/M3 --probes
# The committed freeze is copied from the study's M3_freeze.json.
env -u PYTHONPATH RAYON_NUM_THREADS=12 uv run --frozen --no-sync python \
    experiments/ese3990/evaluate.py --controller mpc --manifest nominal
uv run --frozen --no-sync python experiments/ese3990/mpc_figures.py \
    runs/ese3990/mpc/M3 --out docs/ese3990/figs

# M4: audit completed main runs, freeze selection, then open test/shift once.
env -u PYTHONPATH uv run --frozen --no-sync python experiments/ese3990/audit_runs.py \
    runs/ese3990/main/ppo_R_s{0,1,2} runs/ese3990/main/sac_R_s{0,1,2} \
    --out runs/ese3990/main/M4_audit.json
env -u PYTHONPATH XLA_FLAGS=--xla_cpu_multi_thread_eigen=false RAYON_NUM_THREADS=12 \
    uv run --frozen --no-sync python experiments/ese3990/compare.py --out runs/ese3990/M4
env -u PYTHONPATH MPLCONFIGDIR=/tmp/ese3990-mpl uv run --frozen --no-sync python \
    experiments/ese3990/comparison_report.py runs/ese3990/M4 --out docs/ese3990
env -u PYTHONPATH MPLCONFIGDIR=/tmp/ese3990-mpl uv run --frozen --no-sync python \
    experiments/ese3990/viz/compose.py docs/ese3990/M4_videos.json \
    --out runs/ese3990/M4/videos

# M4U (U, contract 2): demonstrations, 12 main runs, MPC study, held-out comparison.
uv run --frozen --no-sync python experiments/ese3990/evaluate.py --task U --controller rule_sailor \
    --manifest validation --store runs/ese3990/M4U/eval/store_rule_sailor_validation
runs/ese3990/M4U/main/launch.sh   # the 12 runs frozen in configs/M4U_train_freeze.json
env -u PYTHONPATH RAYON_NUM_THREADS=4 uv run --frozen --no-sync python \
    experiments/ese3990/mpc_study.py --task U --jobs 3 --out runs/ese3990/M4U/mpc
env -u PYTHONPATH RAYON_NUM_THREADS=8 uv run --frozen --no-sync python \
    experiments/ese3990/mpc_study.py --out runs/ese3990/M4U/mpc --probes
env -u PYTHONPATH XLA_FLAGS=--xla_cpu_multi_thread_eigen=false RAYON_NUM_THREADS=6 \
    uv run --frozen --no-sync python experiments/ese3990/m4u_compare.py --out runs/ese3990/M4U/heldout \
    --jobs 6 --thresholds experiments/ese3990/configs/M4U_diagnostics.json
env -u PYTHONPATH uv run --frozen --no-sync python experiments/ese3990/replay_audit.py \
    runs/ese3990/M4U/heldout/store/* --out runs/ese3990/M4U/heldout/replay_audit.json
env -u PYTHONPATH MPLCONFIGDIR=/tmp/ese3990-mpl uv run --frozen --no-sync python \
    experiments/ese3990/m4u_report.py runs/ese3990/M4U/heldout --out docs/ese3990

# M4X (post-hoc, exploratory): V check and freeze before any X run, then X2 on
# validation alone (latency), X1/X1b training, held-out, audit and report.
env -u PYTHONPATH uv run --frozen --no-sync python experiments/ese3990/m4x.py vcheck \
    --out docs/ese3990/M4X_vcheck.json
env -u PYTHONPATH uv run --frozen --no-sync python experiments/ese3990/m4x.py freeze \
    --vcheck docs/ese3990/M4X_vcheck.json
env -u PYTHONPATH RAYON_NUM_THREADS=4 XLA_FLAGS=--xla_cpu_multi_thread_eigen=false uv run --frozen --no-sync \
    python experiments/ese3990/m4x.py run --stage validation --out runs/ese3990/M4X/x2_validation --jobs 3
runs/ese3990/M4X/main/launch.sh   # X1 and X1b: train.py --critic-layernorm, 3 seeds each
env -u PYTHONPATH RAYON_NUM_THREADS=4 XLA_FLAGS=--xla_cpu_multi_thread_eigen=false uv run --frozen --no-sync \
    python experiments/ese3990/m4x.py run --stage heldout --out runs/ese3990/M4X/heldout --jobs 3
env -u PYTHONPATH uv run --frozen --no-sync python experiments/ese3990/replay_audit.py \
    runs/ese3990/M4X/x2_validation/store/* runs/ese3990/M4X/heldout/store/* \
    --out runs/ese3990/M4X/replay_audit.json
env -u PYTHONPATH MPLCONFIGDIR=/tmp/ese3990-mpl uv run --frozen --no-sync python \
    experiments/ese3990/m4x_report.py --out docs/ese3990

# M4P (post-hoc, exploratory, no learning): freeze before any probe episode, then
# the grid (about 90 s on 12 cores), the replay audit and the report.
env -u PYTHONPATH uv run --frozen --no-sync python experiments/ese3990/probe.py freeze
env -u PYTHONPATH RAYON_NUM_THREADS=1 uv run --frozen --no-sync \
    python experiments/ese3990/probe.py run --out runs/ese3990/M4P --jobs 12
env -u PYTHONPATH uv run --frozen --no-sync python experiments/ese3990/replay_audit.py \
    runs/ese3990/M4P/store/* --out docs/ese3990/M4P_replay_audit.json --jobs 12
env -u PYTHONPATH MPLCONFIGDIR=/tmp/ese3990-mpl uv run --frozen --no-sync python \
    experiments/ese3990/probe.py report --run runs/ese3990/M4P --out docs/ese3990 --jobs 12

# PPO: a 100k pilot, then evaluate a checkpoint and plot the curve
uv run --frozen --no-sync python experiments/ese3990/train.py --seed 0 --decisions 100000 \
    --checkpoints 10000,50000,100000 --out runs/ese3990/ppo/R_pilot_s0
uv run --frozen --no-sync python experiments/ese3990/evaluate.py --controller ppo \
    --checkpoint runs/ese3990/ppo/R_pilot_s0/ckpt/0100000 --manifest validation
uv run --frozen --no-sync python experiments/ese3990/plots.py runs/ese3990/ppo/R_pilot_s0 \
    --out runs/ese3990/figs/ppo_curve.png

# SAC: same entry point, same run-directory layout
uv run --frozen --no-sync python experiments/ese3990/train.py --learner sac --seed 0 \
    --decisions 100000 --checkpoints 10000,50000,100000 --out runs/ese3990/sac/R_pilot_s0
uv run --frozen --no-sync python experiments/ese3990/evaluate.py --controller sac \
    --checkpoint runs/ese3990/sac/R_pilot_s0/ckpt/0100000 --manifest validation

# log episodes for replay, then render them (plan §10)
uv run --frozen --no-sync python experiments/ese3990/evaluate.py --task U --controller rule_sailor \
    --manifest validation --store runs/ese3990/stores/U_rule
uv run --frozen --no-sync python experiments/ese3990/viz/show.py runs/ese3990/stores/U_rule \
    --manifest validation --limit 8 --out runs/ese3990/figs --name U_val \
    --gif --mp4 --timeseries --speed 6
```

`runs/` is gitignored; small summaries go under `docs/ese3990/`. A store
refuses to mix builds, and replay refuses a different physics build or
`tasks.json`, because exact replay is only valid on the build that recorded
the episode. Set `MPLCONFIGDIR` if `~/.cache/matplotlib` is not writable.

## The contract, in one place

* **Episode.** `Disabled` autoreset; explicit resets; `max_steps` derived from
  the simulated-time budget and `dt` (read from Rust) and checked divisible by
  the cadence.
* **Action.** Learners emit `(rudder_rate, sheet_rate)` in `[-1, 1]`;
  `ActionAdapter` appends release `-1`. RuleSailor's own release flag is
  counted and overwritten.
* **State.** `RawEnv.research_state` — the 13 F3 scalars, controls, capsize
  pending duration, capsize flag and true wind at the boat (names, units and
  version from `sailgym_core`). A readout, not a snapshot.
* **Features.** `sailing.features`: target-relative position, sin/cos yaw and
  boom, unwrapped heel, velocities and rates, actuator state, capsize pending
  duration, wind. No absolute time.
* **Reward.** `-Δt/30 + 10·finished - 10·failed + γΦ(s') - Φ(s)`,
  `Φ = -d(gate centre)/30`, `γ = 0.999`; `Φ = 0` at finish/termination, kept at
  truncation. Scalar and batch paths share one formula.
* **Masks.** `(bootstrap, continue)`: running `(1,1)`, truncated `(1,0)`,
  finished/terminated `(0,0)`.
* **Branching.** `RawEnv.fork()` → an independent `RawEnv` continuing from
  exactly this point under `Disabled`, through the same Rust step path.
  `TaskEnv.rollout(rates (K,H,2))` runs K such branches in Rust and scores
  them with the same reward.
* **Normalization.** 10k smooth-random calibration decisions per training
  seed → per-feature mean/std (floor 0.01), inputs clipped to ±5; saved in
  every checkpoint. SAC also puts those 10k transitions in replay: they are
  its exploration warmup, and updates start right after them.
