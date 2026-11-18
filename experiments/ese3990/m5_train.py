"""M5 PPO/SAC collection at a changed decision interval/start distribution.

Numerical algorithms remain the frozen ppo.py and sac.py. Budgets/checkpoints
count actual 20 Hz decisions, calibration included; optimizer schedules count
learner blocks. Test/shift episodes are never evaluated here.
"""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path

import jax
import numpy as np
import ppo
import sac
import train
from m5 import (
    CELLS,
    FREEZE,
    LABEL,
    BlockCollector,
    HeldAction,
    StartSampler,
    block_gae,
    calibration_blocks,
)
from make_conditions import TrainingSampler
from sailing import HERE, Task, TransitionReward, run_metadata
from viz.episode_store import EpisodeWriter


def config(learner, cell, box, budget=4_000_000):
    cfg = dict(
        json.loads((HERE / "configs/M4U_train_freeze.json").read_text())["configs"][
            learner
        ]
    )
    hold, starts = CELLS[cell]
    if cell != "neither":
        cfg["m5"] = {
            "cell": cell,
            "hold": hold,
            "starts": starts,
            "box": list(box),
            "label": LABEL,
            "short_final_minibatch": "min(configured size, final rollout)",
        }
    cfg["decisions"] = budget
    if budget != 4_000_000:
        if budget == 40_000_000:
            cfg["checkpoints"] = sorted(
                set([*cfg["checkpoints"], 10_000_000, 20_000_000, 30_000_000, budget])
            )
        else:
            cfg["checkpoints"] = [0, budget]
    return cfg


def checkpoint(out, task, writer, learner, params, meta):
    path = out / "ckpt" / f"{meta['decisions']:07d}"
    ppo.save(path, params, meta)
    cls = ppo.PPOPolicy if learner == "ppo" else sac.SACPolicy
    policy = HeldAction(cls(params, meta, str(path)), meta["config"]["m5"]["hold"])
    row = train.evaluate_checkpoint(task, policy, meta["decisions"], writer)
    row["label"] = LABEL
    train.append_jsonl(out / "curve.jsonl", row)
    print(
        f"[ckpt {meta['decisions']}] validation finishes {row['val_finished']}/20",
        flush=True,
    )


def run(learner, seed, out, cfg):
    if "m5" not in cfg:
        # The neither cell's new seeds use the original M4U code/config exactly.
        return (train.train if learner == "ppo" else train.train_sac)(seed, out, cfg)
    out.mkdir(parents=True, exist_ok=False)
    task = Task.load("U")
    gamma = TransitionReward(task).gamma
    hold = cfg["m5"]["hold"]
    block_gamma = gamma**hold
    meta0 = run_metadata(
        task,
        learner=learner,
        training_seed=seed,
        config=cfg,
        label=LABEL,
        probe_ids=list(train.PROBE_IDS),
        gamma_per_block=block_gamma,
    )
    (out / "config.json").write_text(json.dumps(meta0, indent=1) + "\n")
    t0 = time.perf_counter()
    timing = dict(collect_s=0.0, update_s=0.0, eval_s=0.0)
    with EpisodeWriter(out / "store", meta0) as writer:
        sampler = TrainingSampler(seed)
        norm, calibration = train.calibrate(task, seed, cfg, sampler, writer)
        decisions = cfg["calibration_decisions"]
        normalize = lambda rs: ppo.normalize(  # noqa: E731
            rs, task.target, norm, cfg["norm_clip"]
        )
        rng = np.random.default_rng([seed, 1])
        key = jax.random.PRNGKey(seed)
        if cfg["m5"]["starts"]:
            started = StartSampler(seed, cfg["m5"]["box"])
            started.wind = sampler  # continue the wind stream after calibration
            sampler = started
        col = BlockCollector(
            task,
            sampler,
            writer,
            {"controller": learner, "kind": "training"},
            "float32",
            hold,
        )
        if learner == "ppo":
            params = ppo.init(key, len(norm["mean"]), 2, cfg["hidden"], cfg["log_std"])
            opt, update = ppo.make_update(cfg)
            opt_state = opt.init(params)
            mean_fn = jax.jit(lambda p, o: ppo.mlp(p["actor"], o))
            value_fn = jax.jit(ppo.value)
            log_prob_fn = jax.jit(ppo.log_prob)
        else:
            params = sac.init(key, len(norm["mean"]), 2, cfg["hidden"], cfg["alpha0"])
            init_opt, update = sac.make_update(cfg, block_gamma)
            state = dict(
                params=params,
                target_q=params["q"],
                opt=init_opt(params),
                key=jax.random.fold_in(key, 1),
            )
            replay = sac.Replay(cfg["replay"], len(norm["mean"]))

            def store(data):
                replay.add(
                    obs=normalize(data["rs"]),
                    act=data["action"][:, :2],
                    reward=data["reward"],
                    next_obs=normalize(data["next_rs"]),
                    # sac.soft_target multiplies by block_gamma. This
                    # factor gives partial blocks their actual discount.
                    bootstrap=data["bootstrap"] * data["discount"] / block_gamma,
                    cont=data["cont"],
                )

            store(calibration_blocks(calibration, hold, gamma))
            dist_fn = jax.jit(sac.actor_dist)

        pending = sorted(cfg["checkpoints"])
        blocks = updates = 0
        window = []
        ended_window = []
        while True:
            while pending and decisions >= pending[0]:
                pending.pop(0)
                ts = time.perf_counter()
                meta = {
                    **meta0,
                    "normalization": norm,
                    "obs_dim": len(norm["mean"]),
                    "decisions": decisions,
                    "blocks": blocks,
                    "updates": updates,
                    "wall_s": ts - t0,
                }
                checkpoint(out, task, writer, learner, params, meta)
                writer.flush()
                timing["eval_s"] += time.perf_counter() - ts
            if decisions >= cfg["decisions"]:
                break
            ts = time.perf_counter()
            if learner == "ppo":
                std = np.exp(np.asarray(params["log_std"]))

                def choose(rs, params=params, std=std):
                    u = (
                        np.asarray(mean_fn(params, normalize(rs)))
                        + std * rng.standard_normal(2)
                    ).astype(np.float32)
                    return np.tanh(u), u

                data, ended = col.run(
                    cfg["rollout"], choose, max_decisions=cfg["decisions"] - decisions
                )
            else:

                def choose(rs, state=state):
                    mu, log_std = (
                        np.asarray(v)
                        for v in dist_fn(state["params"]["actor"], normalize(rs))
                    )
                    u = (mu + np.exp(log_std) * rng.standard_normal(2)).astype(
                        np.float32
                    )
                    return np.tanh(u), 0.0

                data, ended = col.run(
                    1, choose, max_decisions=cfg["decisions"] - decisions
                )
            decisions += int(data["raw_decisions"].sum())
            blocks += len(data["reward"])
            ended_window.extend(ended)
            timing["collect_s"] += time.perf_counter() - ts
            ts = time.perf_counter()
            if learner == "ppo":
                obs, next_obs = normalize(data["rs"]), normalize(data["next_rs"])
                v = np.asarray(value_fn(params, obs))
                vn = np.asarray(value_fn(params, next_obs))
                adv, ret = block_gae(data, v, vn, cfg["lam"])
                u = data["extra"]
                batch = dict(
                    obs=obs,
                    u=u,
                    logp=np.asarray(log_prob_fn(params, obs, u)),
                    adv=adv.astype(np.float32),
                    ret=ret.astype(np.float32),
                )
                apply_update = update
                if len(adv) < cfg["minibatch"]:
                    _, apply_update = ppo.make_update({**cfg, "minibatch": len(adv)})
                params, opt_state, stats = apply_update(params, opt_state, batch, rng)
                stats.update(
                    std=np.exp(np.asarray(params["log_std"])).tolist(),
                    saturation=float(np.mean(np.abs(np.tanh(u)) > 0.99)),
                )
                window.append(stats)
                updates += 1
            else:
                store(data)
                for _ in range(cfg["utd"]):
                    state, stats = update(state, replay.sample(rng, cfg["minibatch"]))
                    window.append(stats)
                    updates += 1
                params = state["params"]
            timing["update_s"] += time.perf_counter() - ts
            log = learner == "ppo" or blocks % cfg["log_every"] == 0
            if log or decisions == cfg["decisions"]:
                if learner == "sac":
                    samples = jax.device_get(window)
                    stats = {
                        k: float(np.mean([s[k] for s in samples])) for k in samples[0]
                    }
                    stats.update(
                        q_max=float(max(s["q_max"] for s in samples)),
                        q_min=float(min(s["q_min"] for s in samples)),
                        q_abs_max=float(max(s["q_abs_max"] for s in samples)),
                        replay_size=replay.size,
                    )
                fin = sum(e["outcome"] == "finished" for e in ended_window)
                train.append_jsonl(
                    out / "metrics.jsonl",
                    dict(
                        label=LABEL,
                        decisions=decisions,
                        blocks=blocks,
                        updates=updates,
                        wall_s=time.perf_counter() - t0,
                        episodes_ended=len(ended_window),
                        train_success_rate=fin / len(ended_window)
                        if ended_window
                        else None,
                        train_mean_return=float(
                            np.mean([e["return"] for e in ended_window])
                        )
                        if ended_window
                        else None,
                        **stats,
                        **timing,
                    ),
                )
                window, ended_window = [], []
        col.close()
    summary = dict(
        label=LABEL,
        decisions=decisions,
        blocks=blocks,
        updates=updates,
        training_episodes=col.episodes,
        wall_s=time.perf_counter() - t0,
        **timing,
    )
    (out / "summary.json").write_text(json.dumps(summary, indent=1) + "\n")
    print(json.dumps(summary, indent=1), flush=True)
    return out


def main():
    ap = argparse.ArgumentParser(__doc__)
    ap.add_argument("--learner", choices=("ppo", "sac"), required=True)
    ap.add_argument("--cell", choices=CELLS, required=True)
    ap.add_argument("--seed", type=int, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--smoke", action="store_true")
    ap.add_argument("--extended", action="store_true")
    ap.add_argument("--box", type=float, nargs=4)
    args = ap.parse_args()
    if args.smoke:
        if args.seed != 100 or args.extended or args.box is None:
            ap.error("smoke uses seed 100 and an explicit checked box")
        cfg = config(args.learner, args.cell, args.box, 100_000)
    else:
        from m5_study import verify_freeze

        frozen = verify_freeze(FREEZE)
        if args.box is not None:
            ap.error("main runs use the frozen box")
        if args.extended and (args.learner != "ppo" or args.cell not in ("B", "BS")):
            ap.error("only PPO B and BS have extended budgets")
        cfg = config(
            args.learner,
            args.cell,
            frozen["start_box"],
            40_000_000 if args.extended else 4_000_000,
        )
        if args.seed not in frozen["training_seeds"]:
            ap.error("seed outside freeze")
        expected = frozen["configs"][
            f"{args.learner}_{args.cell}" + ("_extended" if args.extended else "")
        ]
        if cfg != expected:
            raise ValueError("config differs from M5 freeze")
    run(args.learner, args.seed, args.out, cfg)


if __name__ == "__main__":
    main()
