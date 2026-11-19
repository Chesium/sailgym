"""Train M6 with the frozen single-environment PPO/SAC updates and M6 resets."""

import argparse
import time

import jax
import numpy as np
import ppo
import sac
import train
from compare import digest
from m5 import StartTask
from m6 import FINAL, ROOT, atomic_json, draw, panel, read, score
from make_conditions import TrainingSampler
from sailing import HERE, Task, TaskEnv, TransitionReward, run_metadata
from viz.episode_store import EpisodeRecorder, EpisodeWriter


class CurriculumCollector(train.Collector):
    def _begin(self):
        state = self.curriculum
        stage = state.stage
        if 0 < stage < FINAL and state.rng.random() < 0.2:
            stage -= 1
        cond, xy = draw(self.sampler, state.rng, stage, state.freeze["widths"][stage])
        self.env = TaskEnv(StartTask.at(self.task, xy), cond, self.episodes)
        self.rs = self.env.reset()
        self.recorder = EpisodeRecorder(self.env, self.dtype)
        self.recorder.start(self.rs)
        self.meta.update(stage=stage, m5_start_xy=xy)
        self.ret = 0.0

    def __init__(self, state, *args):
        self.curriculum = state
        super().__init__(*args)

    def _log(self):
        super()._log()
        if self.env.outcome != "running":
            self.curriculum.ended(self.meta["stage"], self.recorder._k)


class Curriculum:
    def __init__(self, root, out, freeze, seed, learner, task, writer, metadata):
        self.freeze, self.out = freeze, out
        self.seed, self.learner, self.task, self.writer = seed, learner, task, writer
        self.metadata = metadata
        self.rng = np.random.default_rng([seed, 6])
        self.stage = self.episodes = self.streak = self.round = 0
        self.probe_s = 0
        self.completed_steps = metadata["config"]["calibration_decisions"]
        self.qualified = False
        self.params = None

    def policy(self, decisions, path):
        meta = {**self.metadata, "decisions": decisions}
        cls = ppo.PPOPolicy if self.learner == "ppo" else sac.SACPolicy
        return cls(self.params, meta, str(path))

    def probe(self, kind, stage, rows, decisions):
        path = self.out / "probes" / f"{decisions:07d}"
        policy = self.policy(decisions, path)
        if not path.with_suffix(".npz").exists():
            ppo.save(path, self.params, policy.meta)
        ts = time.monotonic()
        row = score(self.task, self.writer, policy, rows, kind, stage, decisions)
        row.update(
            kind=kind, stage=stage, checkpoint=decisions, wall_s=time.monotonic() - ts
        )
        self.probe_s += row["wall_s"]
        train.append_jsonl(self.out / "probes.jsonl", row)
        return row

    def ended(self, stage, steps):
        self.completed_steps += steps
        if stage != self.stage:
            return
        self.episodes += 1
        if self.episodes % 25:
            return
        self.round += 1
        seed = 1000000 + self.seed * 10000 + self.round
        rows = panel(stage, seed, self.freeze["widths"][stage])
        row = self.probe("promotion", stage, rows, self.completed_steps)
        n = row["finishes"]
        if stage == 1 and n > 0:
            self.qualified = True
        controls = self.freeze["controls"][stage].values()
        passes = n >= 6 and all(
            n / 20 > r["finishes"] / r["attempts"] for r in controls
        )
        self.streak = self.streak + 1 if passes else 0
        if self.streak < 2 or stage == FINAL:
            return
        if stage > 0 and self.freeze["paired"][stage]:
            self.probe(
                "paired_after",
                stage,
                self.freeze["paired"][stage],
                self.completed_steps,
            )
        next_stage = stage + 1
        paired = self.freeze["paired"][next_stage]
        if paired:
            self.probe("paired_before", next_stage, paired, self.completed_steps)
        elif next_stage == FINAL:
            self.probe(
                "original_before",
                FINAL,
                panel(FINAL, seed + 5000, 0),
                self.completed_steps,
            )
        self.stage, self.streak, self.episodes = next_stage, 0, 0
        self.probe(
            "retention",
            stage,
            panel(stage, seed + 5000, self.freeze["widths"][stage]),
            self.completed_steps,
        )
        train.append_jsonl(
            self.out / "promotions.jsonl",
            dict(stage=next_stage, decisions=self.completed_steps),
        )


def run(root, learner, seed, main=False):
    freeze = read(root / "freeze.json")
    for file, checksum in freeze["sources"].items():
        path = HERE / file
        if not path.exists():
            path = HERE / "configs" / file
        if digest(path) != checksum:
            raise ValueError(f"changed frozen input {path}")
    clock = read(root / "clock.json")["start"]
    deadline = clock + (110 if main else 50) * 60
    out = root / ("main" if main else "pilot") / f"{learner}_s{seed}"
    out.mkdir(parents=True, exist_ok=False)
    task = Task.load("U")
    cfg = read(HERE / "configs/M4U_train_freeze.json")["configs"][learner].copy()
    if not main:
        cfg.update(decisions=400000, checkpoints=[400000])
    meta = run_metadata(
        task,
        learner=learner,
        training_seed=seed,
        config=cfg,
        study="M6",
        freeze_sha256=digest(root / "freeze.json"),
    )
    atomic_json(out / "config.json", meta)
    start = time.monotonic()
    with EpisodeWriter(out / "store", meta, shard_size=24) as writer:
        wind = TrainingSampler(seed)
        norm, cal = train.calibrate(task, seed, cfg, wind, writer)
        meta.update(normalization=norm, obs_dim=len(norm["mean"]))
        state = Curriculum(root, out, freeze, seed, learner, task, writer, meta)
        col = CurriculumCollector(
            state,
            task,
            wind,
            writer,
            dict(controller=learner, kind="training"),
            "float32",
        )

        def normalize(rs):
            return ppo.normalize(rs, task.target, norm, cfg["norm_clip"])

        gamma = TransitionReward(task).gamma
        rng = np.random.default_rng([seed, 1])
        key = jax.random.PRNGKey(seed)
        if learner == "ppo":
            params = ppo.init(key, meta["obs_dim"], 2, cfg["hidden"], cfg["log_std"])
            opt, update = ppo.make_update(cfg)
            opt_state = opt.init(params)
            actor = jax.jit(lambda p, o: ppo.mlp(p["actor"], o))
            value, logp = jax.jit(ppo.value), jax.jit(ppo.log_prob)

            def choose(rs):
                u = (
                    np.asarray(actor(params, normalize(rs)))
                    + std * rng.standard_normal(2)
                ).astype(np.float32)
                return np.tanh(u), u
        else:
            params = sac.init(key, meta["obs_dim"], 2, cfg["hidden"], cfg["alpha0"])
            init_opt, update = sac.make_update(cfg, gamma)
            opt_state = dict(
                params=params,
                target_q=params["q"],
                opt=init_opt(params),
                key=jax.random.fold_in(key, 1),
            )
            replay = sac.Replay(cfg["replay"], meta["obs_dim"])

            def store(data):
                replay.add(
                    obs=normalize(data["rs"]),
                    act=data["action"][:, :2],
                    reward=data["reward"],
                    next_obs=normalize(data["next_rs"]),
                    bootstrap=data["bootstrap"],
                    cont=data["cont"],
                )

            store(cal)
            dist = jax.jit(sac.actor_dist)

            def choose(rs):
                mu, ls = map(np.asarray, dist(params["actor"], normalize(rs)))
                u = (mu + np.exp(ls) * rng.standard_normal(2)).astype(np.float32)
                return np.tanh(u), 0.0

        decisions = cfg["calibration_decisions"]
        pending = sorted(cfg["checkpoints"])
        state.params = params
        updates = 0
        q_window = []
        while True:
            done = decisions >= cfg["decisions"] or time.time() >= deadline
            if (pending and decisions >= pending[0]) or done:
                if pending and decisions >= pending[0]:
                    pending.pop(0)
                path = out / "ckpt" / f"{decisions:07d}"
                policy = state.policy(decisions, path)
                ppo.save(path, params, policy.meta)
                if main:
                    row = train.evaluate_checkpoint(task, policy, decisions, writer)
                    train.append_jsonl(out / "curve.jsonl", row)
                writer.flush()
            if done:
                break
            n = min(cfg.get("rollout", 1), cfg["decisions"] - decisions)
            if learner == "ppo":
                std = np.exp(np.asarray(params["log_std"]))
            data, _ = col.run(n, choose)
            decisions += n
            if learner == "ppo":
                obs, nxt = normalize(data["rs"]), normalize(data["next_rs"])
                v, vn = np.asarray(value(params, obs)), np.asarray(value(params, nxt))
                adv, ret = ppo.gae(
                    data["reward"],
                    v,
                    vn,
                    data["bootstrap"],
                    data["cont"],
                    gamma,
                    cfg["lam"],
                )
                u = data["extra"]
                batch = dict(
                    obs=obs,
                    u=u,
                    logp=np.asarray(logp(params, obs, u)),
                    adv=adv.astype(np.float32),
                    ret=ret.astype(np.float32),
                )
                step = (
                    update
                    if n >= cfg["minibatch"]
                    else ppo.make_update({**cfg, "minibatch": n})[1]
                )
                params, opt_state, stats = step(params, opt_state, batch, rng)
            else:
                store(data)
                assert cfg["utd"] == 1
                opt_state, stats = update(
                    opt_state, replay.sample(rng, cfg["minibatch"])
                )
                params = opt_state["params"]
                q_window.append((stats["q_min"], stats["q_max"]))
            state.params = params
            updates += 1
            if learner == "ppo" or updates % 2048 == 0 or decisions == cfg["decisions"]:
                metrics = dict(
                    decisions=decisions,
                    stage=state.stage,
                    episodes=col.episodes,
                    elapsed_s=time.monotonic() - start,
                    probe_s=state.probe_s,
                )
                if q_window:
                    q = np.asarray(jax.device_get(q_window))
                    metrics.update(
                        q_min=float(q[:, 0].min()), q_max=float(q[:, 1].max())
                    )
                    q_window.clear()
                train.append_jsonl(out / "metrics.jsonl", metrics)
        col.close()
        if state.stage > 0 and freeze["paired"][state.stage]:
            state.probe(
                "paired_after", state.stage, freeze["paired"][state.stage], decisions
            )
        if state.stage > 0:
            prev = state.stage - 1
            state.probe(
                "final_retention",
                prev,
                panel(prev, 800000 + seed, freeze["widths"][prev]),
                decisions,
            )
    atomic_json(
        out / "summary.json",
        dict(
            decisions=decisions,
            stage=state.stage,
            pilot_pass=state.qualified,
            updates=updates,
            probe_s=state.probe_s,
            wall_s=time.monotonic() - start,
            episodes=col.episodes,
            complete=decisions == cfg["decisions"],
        ),
    )


if __name__ == "__main__":
    p = argparse.ArgumentParser(__doc__)
    p.add_argument("learner", choices=["ppo", "sac"])
    p.add_argument("seed", type=int)
    p.add_argument("--main", action="store_true")
    a = p.parse_args()
    run(ROOT, a.learner, a.seed, a.main)
