"""M5b single-change training arms. Each invocation owns one training process."""

import argparse
import json
import math
import time
from unittest.mock import patch

import jax
import jax.numpy as jnp
import numpy as np
import ppo
import sac
import train
from m5_study import verify_freeze
from probe import powerlaw_noise
from sailing import HERE, REPO, Task, TransitionReward, run_metadata

ROOT = REPO / "runs/ese3990/M5b"
FREEZE = HERE / "configs/M5b_freeze.json"
ARMS = {"SAC-E": "sac", "PPO-8": "ppo", "SAC-P": "sac", "PPO-sigma": "ppo"}
LABEL = "M5b upwind without demonstrations (post-hoc, exploratory)"


def config(arm, parent, smoke=False):
    learner = ARMS[arm]
    cfg = dict(parent["configs"][f"{learner}_neither"], arm=arm, label=LABEL)
    if arm == "PPO-sigma":
        cfg["log_std"] = math.log(0.3)
    if smoke:
        cfg.update(decisions=100_000, checkpoints=[0, 100_000])
    return cfg


def task_target(params, target_q, batch, key, gamma):
    action, _ = sac.sample_action(params["actor"], batch["next_obs"], key)
    value = jnp.min(sac.q_values(target_q, batch["next_obs"], action), axis=0)
    return jax.lax.stop_gradient(batch["reward"] + gamma * batch["bootstrap"] * value)


class EightCollectors:
    def __init__(self, lanes):
        self.lanes = lanes

    @property
    def episodes(self):
        return sum(c.episodes for c in self.lanes)

    def run(self, n, choose):
        parts, ended = [], []
        for i, lane in enumerate(self.lanes):
            size = n // len(self.lanes) + (i < n % len(self.lanes))
            if size:
                data, done = lane.run(size, choose)
                data["cont"][-1] = 0  # end this lane's GAE trace; keep V(next)
                parts.append(data)
                ended.extend(done)
        return {k: np.concatenate([p[k] for p in parts]) for k in parts[0]}, ended

    def close(self):
        for lane in self.lanes:
            lane.close()


def pink_sequence(rng, length):
    return np.stack([powerlaw_noise(rng, 1, length) for _ in range(2)], axis=1)


def train_pink(seed, out, cfg):
    out.mkdir(parents=True, exist_ok=False)
    task = Task.load("U")
    meta = run_metadata(
        task,
        learner="sac",
        training_seed=seed,
        config=cfg,
        sac_version=sac.SAC_VERSION,
        probe_ids=list(train.PROBE_IDS),
    )
    (out / "config.json").write_text(json.dumps(meta, indent=1))
    sampler = train.TrainingSampler(seed)
    rng = np.random.default_rng([seed, 1])
    noise_rng = np.random.default_rng([seed, 2])
    t0 = time.perf_counter()
    timing = dict(collect_s=0.0, update_s=0.0, eval_s=0.0)
    with train.EpisodeWriter(out / "store", meta) as writer:
        norm, calibration = train.calibrate(task, seed, cfg, sampler, writer)

        def normalize(rs):
            return ppo.normalize(rs, task.target, norm, cfg["norm_clip"])

        replay = sac.Replay(cfg["replay"], len(norm["mean"]))

        def store(data):
            replay.add(
                obs=normalize(data["rs"]),
                act=data["action"][:, :2],
                next_obs=normalize(data["next_rs"]),
                reward=data["reward"],
                bootstrap=data["bootstrap"],
                cont=data["cont"],
            )

        store(calibration)
        key = jax.random.PRNGKey(seed)
        params = sac.init(key, len(norm["mean"]), 2, cfg["hidden"], cfg["alpha0"])
        init, update = sac.make_update(cfg, TransitionReward(task).gamma)
        state = dict(
            params=params,
            target_q=params["q"],
            opt=init(params),
            key=jax.random.fold_in(key, 1),
        )
        mean_action = jax.jit(sac.mean_action)
        collector = train.Collector(
            task, sampler, writer, dict(controller="sac", kind="training"), "float32"
        )
        noise = pink_sequence(noise_rng, task.max_decisions)
        k = 0

        def choose(rs):
            mean = np.asarray(mean_action(state["params"], normalize(rs)))
            return np.clip(mean + 0.3 * noise[k], -1.0, 1.0).astype(np.float32), 0.0

        pending = list(cfg["checkpoints"])
        decisions = cfg["calibration_decisions"]
        window, ended = [], []
        while True:
            if pending and decisions >= pending[0]:
                pending.pop(0)
                start = time.perf_counter()
                train.checkpoint(
                    out,
                    task,
                    writer,
                    sac.SACPolicy,
                    state["params"],
                    dict(
                        meta,
                        decisions=decisions,
                        obs_dim=len(norm["mean"]),
                        normalization=norm,
                        wall_s=start - t0,
                    ),
                )
                timing["eval_s"] += time.perf_counter() - start
            if decisions == cfg["decisions"]:
                break
            start = time.perf_counter()
            data, done = collector.run(1, choose)
            k += 1
            if done:
                ended.extend(done)
                noise = pink_sequence(noise_rng, task.max_decisions)
                k = 0
            store(data)
            # Preserve the baseline replay minibatch RNG stream: it consumes two
            # Gaussian draws for collection before sampling the next minibatch.
            rng.standard_normal(2)
            decisions += 1
            timing["collect_s"] += time.perf_counter() - start
            start = time.perf_counter()
            for _ in range(cfg["utd"]):
                state, stats = update(state, replay.sample(rng, cfg["minibatch"]))
                window.append(stats)
            timing["update_s"] += time.perf_counter() - start
            if decisions % cfg["log_every"] == 0 or decisions == cfg["decisions"]:
                samples = jax.device_get(window)
                metrics = {
                    k: float(np.mean([s[k] for s in samples])) for k in samples[0]
                }
                for name, agg in (("q_min", min), ("q_max", max), ("q_abs_max", max)):
                    metrics[name] = float(agg(s[name] for s in samples))
                train.append_jsonl(
                    out / "metrics.jsonl",
                    dict(
                        metrics,
                        **timing,
                        decisions=decisions,
                        wall_s=time.perf_counter() - t0,
                        replay_size=replay.size,
                        episodes_ended=len(ended),
                        train_success_rate=(
                            sum(e["outcome"] == "finished" for e in ended) / len(ended)
                            if ended
                            else None
                        ),
                        train_mean_return=float(np.mean([e["return"] for e in ended]))
                        if ended
                        else None,
                    ),
                )
                window, ended = [], []
        collector.close()
    (out / "summary.json").write_text(
        json.dumps(
            dict(
                timing,
                decisions=decisions,
                wall_s=time.perf_counter() - t0,
                training_episodes=collector.episodes,
            ),
            indent=1,
        )
    )


def run(arm, seed, out, cfg):
    if arm == "SAC-E":
        # The original optimizer resolves the target when JAX traces its update.
        with patch.object(sac, "soft_target", task_target):
            return train.train_sac(seed, out, cfg)
    if arm == "SAC-P":
        return train_pink(seed, out, cfg)
    if arm == "PPO-8":
        scalar = train.Collector

        def collectors(task, sampler, writer, meta, dtype):
            if meta["kind"] != "training":
                return scalar(task, sampler, writer, meta, dtype)
            return EightCollectors(
                [
                    scalar(task, sampler, writer, dict(meta, lane=i), dtype)
                    for i in range(8)
                ]
            )

        with patch.object(train, "Collector", collectors):
            return train.train(seed, out, cfg)
    return train.train(seed, out, cfg)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage", choices=["smoke", "main"])
    parser.add_argument("arm", choices=ARMS)
    parser.add_argument("seed", type=int)
    args = parser.parse_args()
    smoke = args.stage == "smoke"
    parent = verify_freeze()
    cfg = config(args.arm, parent, smoke)
    if smoke:
        if args.seed != 100:
            parser.error("pilot seed must be 100")
    else:
        from m5b_study import verify

        frozen = verify()
        if (
            args.seed not in frozen["training_seeds"]
            or cfg != frozen["configs"][f"{ARMS[args.arm]}_{args.arm}"]
        ):
            parser.error("seed or config differs from freeze")
    run(args.arm, args.seed, ROOT / args.stage / f"{args.arm}_s{args.seed}", cfg)
