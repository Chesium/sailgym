# ESE3990 experiments

Learning versus planning on SailGym: PPO, SAC and true-model CEM-MPC, with
RuleSailor as an engineering reference. The plan is
[`docs/ese3990/ESE3990_SailGym_Project_Plan.md`](../../docs/ese3990/ESE3990_SailGym_Project_Plan.md);
the M0 evidence is in [`docs/ese3990/M0_report.md`](../../docs/ese3990/M0_report.md).

Physics, gate passage, capsize, bounds and time limits are Rust decisions
(`sailgym_core`). Nothing in this directory models the boat.

## Layout

| File | Role |
|---|---|
| `configs/tasks.json` | Tasks R and U, bounds, cadence, action restriction, reward constants |
| `configs/conditions/*.json` | Generated condition manifests (validation 20, test 50, shift 20; training sampler spec) |
| `make_conditions.py` | Deterministic manifest generator; `--check` verifies the committed files |
| `sailing.py` | Shared contract: `Task`, `Condition`, `ActionAdapter`, `features`, `TransitionReward`, `masks`, `TaskEnv`, `run_metadata` |
| `baselines.py` | `SmoothRandom` and `RuleSailorController` (bound Rust RuleSailor, release forced off) |
| `evaluate.py` | One evaluator for every controller; writes an outcome report |
| `viz/episode_store.py` | Episode logs (actions + condition + seed + build identity + checksum + 5 Hz pose) in compressed shards with an `index.jsonl` (plan §10) |
| `viz/replay.py` | Exact re-simulation of a stored episode, verified against the log; pose-only fallback |
| `viz/render.py` | Static track figure, time series, and a synced multi-episode gif/mp4 overlay |
| `viz/show.py` | Command-line entry: select episodes from a store, replay them and render |
| `tests/` | Contract, branch, baseline and viz checks |

Not yet written (M1+): `ppo.py`, `sac.py`, `mpc.py`, `train.py`, `plots.py`, `viz/compose.py`.

## Commands

Run from the repository root. Unset `PYTHONPATH` if a ROS (or other) install
puts its packages on it — `scripts/py-test.sh` does the same.

```bash
# build the extension after any Rust change
uv run maturin develop --uv --release --manifest-path crates/sailgym-py/Cargo.toml

# experiment tests (not part of the default `pytest python` discovery)
env -u PYTHONPATH uv run pytest experiments/ese3990/tests

# Rust branch-vs-replay proofs
cargo test -p sailgym-env --test fork

# verify the committed manifests
uv run python experiments/ese3990/make_conditions.py --check

# an outcome report
uv run python experiments/ese3990/evaluate.py --task R --controller rule_sailor \
    --manifest validation --out runs/ese3990/eval/R_rule_validation.json

# log episodes for replay, then render them (plan §10)
uv run python experiments/ese3990/evaluate.py --task U --controller rule_sailor \
    --manifest validation --store runs/ese3990/stores/U_rule
uv run python experiments/ese3990/viz/show.py runs/ese3990/stores/U_rule \
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
