"""M6 opening-policy handoff and positive-advantage self-imitation campaign."""

import os
import subprocess
import sys
import time
from pathlib import Path
from unittest.mock import patch

import jax
import jax.numpy as jnp
import m6_train
import numpy as np
import optax
import ppo
import train
from compare import digest
from m6 import atomic_json, audit, panel, read, score
from m6_campaign import lines
from m6_width import ROOT as WIDTH
from m6_width import SEEDS, SOURCE, FirstBand
from sailing import Task, TransitionReward, load_manifest
from viz.episode_store import EpisodeStore, EpisodeWriter

ROOT = WIDTH.parent / "imitation"
BASE = WIDTH / "broad/pilot"
EARLY, LATE = "0077991", "0400000"
COEFFICIENT = 0.1
FRESH_SEED = 616101


class Handoff:
    action_dtype = "float32"

    def __init__(self, opening, following, opening_steps):
        self.opening, self.following, self.opening_steps = (
            opening,
            following,
            opening_steps,
        )

    def reset(self, env):
        self.count = 0
        self.opening.reset(env)
        self.following.reset(env)

    def act(self, env):
        policy = self.opening if self.count < self.opening_steps else self.following
        self.count += 1
        return policy.act(env)

    def identity(self):
        return dict(
            controller="ppo_handoff",
            opening=self.opening.identity(),
            following=self.following.identity(),
            opening_steps=self.opening_steps,
        )


class SuccessBuffer:
    """Receive online collector batches only. Paths may span any number of rollouts."""

    def __init__(self, gamma):
        self.gamma = gamma
        self.pending = []
        self.rows = None
        self.wins = []

    def collect(self, obs, u, reward, cont, ended, start_decisions):
        cut = np.flatnonzero(cont == 0) + 1
        begin = 0
        for stop, episode in zip(cut, ended, strict=True):
            self.pending.append((obs[begin:stop], u[begin:stop], reward[begin:stop]))
            if episode["outcome"] == "finished":
                xs, us, rs = map(np.concatenate, zip(*self.pending, strict=True))
                returns = np.empty(len(rs), np.float32)
                future = 0.0
                for i in reversed(range(len(rs))):
                    future = rs[i] + self.gamma * future
                    returns[i] = future
                rows = dict(obs=xs, u=us, returns=returns)
                self.rows = (
                    rows
                    if self.rows is None
                    else {k: np.concatenate((v, rows[k])) for k, v in self.rows.items()}
                )
                self.wins.append(
                    dict(decisions=int(start_decisions + stop), length=len(rs))
                )
            self.pending = []
            begin = stop
        if begin < len(reward):
            self.pending.append((obs[begin:], u[begin:], reward[begin:]))

    def sample(self, rng, n):
        ix = rng.integers(len(self.rows["returns"]), size=n)
        return {k: v[ix] for k, v in self.rows.items()}


def imitation_loss(params, replay):
    weight = jax.lax.stop_gradient(
        jnp.maximum(replay["returns"] - ppo.value(params, replay["obs"]), 0)
    )
    return -(
        weight * ppo.log_prob(params, replay["obs"], replay["u"])
    ).mean(), weight.mean()


def joint_step(cfg, opt):
    def loss(params, batch, replay):
        ordinary, _ = ppo.loss(params, batch, cfg)
        auxiliary, weight = imitation_loss(params, replay)
        return ordinary + COEFFICIENT * auxiliary, weight

    @jax.jit
    def step(params, opt_state, batch, replay):
        (_, weight), grads = jax.value_and_grad(loss, has_aux=True)(
            params, batch, replay
        )
        updates, opt_state = opt.update(grads, opt_state, params)
        return optax.apply_updates(params, updates), opt_state, weight

    return step


def fit(seed):
    task = Task.load("U")
    out = ROOT / "pilot" / f"ppo_s{seed}"
    buffer = SuccessBuffer(TransitionReward(task).gamma)
    replay_rng = np.random.default_rng([seed, 610])
    original_update = ppo.make_update
    anchor = None
    fit_col = None

    class OnlineCollector(m6_train.CurriculumCollector):
        def run(self, n, choose):
            nonlocal fit_col
            fit_col = self
            assert self.meta["kind"] == "training" and self.meta["controller"] == "ppo"
            data, ended = super().run(n, choose)
            meta = self.curriculum.metadata
            obs = ppo.normalize(
                data["rs"],
                task.target,
                meta["normalization"],
                meta["config"]["norm_clip"],
            )
            start = getattr(
                self, "online_decisions", meta["config"]["calibration_decisions"]
            )
            buffer.collect(
                obs, data["extra"], data["reward"], data["cont"], ended, start
            )
            self.online_decisions = start + n
            return data, ended

    def make_update(cfg):
        opt, ordinary = original_update(cfg)
        step = joint_step(cfg, opt)

        def update(params, opt_state, data, rng):
            nonlocal anchor
            if buffer.rows is None:
                return ordinary(params, opt_state, data, rng)
            if anchor is None:
                anchor = fit_col.online_decisions
                ppo.save(
                    out / "anchor",
                    params,
                    {**fit_col.curriculum.metadata, "decisions": anchor},
                )
            weights = []
            mb = cfg["minibatch"]
            for _ in range(cfg["epochs"]):
                perm = rng.permutation(len(data["adv"]))
                for i in range(0, len(perm) - mb + 1, mb):
                    batch = {k: v[perm[i : i + mb]] for k, v in data.items()}
                    params, opt_state, weight = step(
                        params, opt_state, batch, buffer.sample(replay_rng, mb)
                    )
                    weights.append(weight)
            train.append_jsonl(
                out / "imitation.jsonl",
                dict(
                    decisions=fit_col.online_decisions,
                    samples=len(buffer.rows["returns"]),
                    minibatches=len(weights),
                    mean_positive_advantage=float(np.mean(jax.device_get(weights))),
                ),
            )
            return params, opt_state, {}

        return opt, update

    # Run seed-local interventions in a dedicated process. The original trainer's
    # calibration, stochastic policy, RNG, GAE, PPO optimizer and probes stay shared.
    with (
        patch.object(m6_train, "Curriculum", FirstBand),
        patch.object(m6_train, "CurriculumCollector", OnlineCollector),
        patch.object(ppo, "make_update", make_update),
    ):
        m6_train.run(ROOT, "ppo", seed)
    summary = read(out / "summary.json")
    assert summary["complete"] and summary["decisions"] == 400000
    atomic_json(out / "buffer.json", dict(wins=buffer.wins, anchor=anchor))
    verify_prefix(seed, anchor or 400000)
    audit(out)


def verify_prefix(seed, steps):
    """Compare all collected actions through the first affected update."""
    new = ROOT / "pilot" / f"ppo_s{seed}"
    old = BASE / f"ppo_s{seed}"
    a, b = (EpisodeStore(p / "store") for p in (new, old))

    def trajectories(s):
        return (e for e in s.index if e["kind"] in ("calibration", "training"))

    remaining = steps
    for x, y in zip(trajectories(a), trajectories(b), strict=False):
        assert all(
            x.get(k) == y.get(k) for k in ("seed", "kind", "m5_start_xy", "condition")
        )
        n = min(remaining, x["decisions"])
        np.testing.assert_array_equal(a.load(x).actions[:n], b.load(y).actions[:n])
        if n == x["decisions"] and remaining > n:
            assert x["final_checksum"] == y["final_checksum"]
        remaining -= n
        if remaining == 0:
            return
    raise ValueError("baseline prefix is shorter than the first update")


def handoff(label):
    protocol = read(ROOT / "protocol.json")
    first, second = label.split("_")
    paths = protocol["handoff_checkpoints"]

    def load(name):
        source = paths[name]
        path = SOURCE / source["path"]
        assert all(
            digest(path.with_suffix(ext)) == source[ext] for ext in (".json", ".npz")
        )
        return ppo.PPOPolicy.load(path)

    policy = Handoff(load(first), load(second), protocol["opening_steps"])
    out = ROOT / "handoff" / label
    with EpisodeWriter(out / "store", policy.identity(), shard_size=16) as writer:
        score(Task.load("U"), writer, policy, protocol["broad"], label, 0)
    audit(out)
    if first == second:
        old = WIDTH / "retention" / (EARLY if first == "early" else LATE)
        a, b = (EpisodeStore(p / "store") for p in (old, out))
        assert len(a.index) == len(b.index) == 80
        for x, y in zip(a.index, b.index, strict=True):
            assert (
                x["condition"] == y["condition"]
                and x["final_checksum"] == y["final_checksum"]
            )
            np.testing.assert_array_equal(a.load(x).actions, b.load(y).actions)


def evaluate(seed):
    protocol = read(ROOT / "protocol.json")
    out = ROOT / "pilot" / f"ppo_s{seed}"
    old = BASE / f"ppo_s{seed}"
    anchor = read(out / "buffer.json")["anchor"]
    task = Task.load("U")
    fixed = protocol["broad"]
    entries = []
    if anchor is not None:
        entries.append(("anchor", out / "anchor", fixed))
    for name, run in [("baseline", old), ("imitation", out)]:
        first_probe = lines(run / "probes.jsonl")[0]["checkpoint"]
        entries.extend(
            [
                (name + "_first_probe", run / "probes" / f"{first_probe:07d}", fixed),
                (name + "_fresh", run / "ckpt/0400000", protocol["fresh"]),
            ]
        )
    entries.extend(
        [
            ("imitation_final", out / "ckpt/0400000", fixed),
            ("original_validation", out / "ckpt/0400000", protocol["validation"]),
        ]
    )
    with EpisodeWriter(out / "evaluation/store", shard_size=20) as writer:
        for name, path, rows in entries:
            policy = ppo.PPOPolicy.load(path)
            score(
                task,
                writer,
                policy,
                rows,
                name,
                0 if name != "original_validation" else 5,
                policy.meta["decisions"],
            )
    audit(out / "evaluation")


def transfer(seed):
    out = ROOT / "pilot" / f"ppo_s{seed}"
    step = read(out / "qualifying.json")["decisions"]
    with EpisodeWriter(out / "transfer/store", shard_size=20) as writer:
        score(
            Task.load("U"),
            writer,
            ppo.PPOPolicy.load(out / "probes" / f"{step:07d}"),
            read(ROOT / "protocol.json")["transfer"],
            "paired_before",
            1,
            step,
        )
    audit(out / "transfer")


def wins(rows, kind):
    return {
        r["condition"] for r in rows if r["kind"] == kind and r["outcome"] == "finished"
    }


def decision():
    winners = []
    totals = {"baseline": 0, "imitation": 0}
    for seed in SEEDS:
        out = ROOT / "pilot" / f"ppo_s{seed}"
        rows = lines(out / "evaluation/audit.jsonl")
        anchors = wins(rows, "anchor")
        orig = lines(BASE.joinpath(f"ppo_s{seed}/evaluation/audit.jsonl"))
        totals["baseline"] += len(anchors & wins(orig, "broad"))
        totals["imitation"] += len(anchors & wins(rows, "imitation_final"))
        if (out / "qualifying.json").exists():
            winners.append(seed)
    run_transfer = len(winners) >= 2 and totals["imitation"] > totals["baseline"]
    return dict(qualified=winners, retained=totals, run_transfer=run_transfer)


def batch(cmd, args):
    cpus = sorted(os.sched_getaffinity(0))
    jobs = []
    for i, arg in enumerate(args):
        ids = cpus[3 * i : 3 * i + 3] or cpus[:3]
        with (ROOT / f"{cmd}_{arg}.log").open("a") as log:
            process = subprocess.Popen(
                [
                    "taskset",
                    "-c",
                    ",".join(map(str, ids)),
                    sys.executable,
                    __file__,
                    cmd,
                    str(arg),
                ],
                stdout=log,
                stderr=log,
            )
        jobs.append((arg, process))
    failures = [arg for arg, p in jobs if p.wait()]
    if failures:
        raise RuntimeError(f"{cmd} failed for {failures}")


def campaign():
    ROOT.mkdir(parents=True, exist_ok=False)
    width = read(WIDTH / "protocol.json")
    first = read(WIDTH / "broad/freeze.json")
    task = Task.load("U")
    steps = round(2 / task.decision_dt)
    assert np.isclose(steps * task.decision_dt, 2)
    protocol = dict(
        start=time.time(),
        seeds=SEEDS,
        coefficient=COEFFICIENT,
        handoff_checkpoints=dict(
            early=width["checkpoints"][EARLY], late=width["checkpoints"][LATE]
        ),
        broad=width["panels"]["broad"],
        fresh=panel(0, FRESH_SEED, 10, 80),
        opening_steps=steps,
        validation=[
            dict(seed=0, xy=[0, 0], condition=c.to_dict())
            for c in load_manifest("validation")
        ],
        transfer=width["transfer"],
        decision="2/3 qualify and pooled retained anchor wins exceed baseline",
        width_freeze=digest(WIDTH / "broad/freeze.json"),
        width_protocol=digest(WIDTH / "protocol.json"),
        baseline_inputs={
            str(p.relative_to(BASE)): digest(p)
            for s in SEEDS
            for p in (BASE / f"ppo_s{s}").rglob("*")
            if p.is_file()
            and (
                p.suffix in (".npz", ".json")
                and p.parent.name in ("probes", "ckpt")
                or p.name in ("index.jsonl", "audit.jsonl", "probes.jsonl")
            )
        },
    )
    atomic_json(ROOT / "protocol.json", protocol)
    atomic_json(
        ROOT / "freeze.json",
        {
            **first,
            "sources": {
                **first["sources"],
                Path(__file__).name: digest(Path(__file__)),
            },
            "self_imitation": dict(
                coefficient=COEFFICIENT, protocol=digest(ROOT / "protocol.json")
            ),
        },
    )
    atomic_json(ROOT / "clock.json", {"start": protocol["start"]})

    def status(phase):
        atomic_json(
            ROOT / "status.json",
            dict(phase=phase, elapsed_s=time.time() - protocol["start"]),
        )

    status("handoff")
    batch("handoff", ["early_early", "early_late", "late_early", "late_late"])
    status("self-imitation training")
    batch("fit", SEEDS)
    status("retention and fresh evaluation")
    batch("evaluate", SEEDS)
    result = decision()
    atomic_json(ROOT / "decision.json", result)
    if result["run_transfer"]:
        batch("transfer", result["qualified"])
    for name, checksum in protocol["baseline_inputs"].items():
        assert digest(BASE / name) == checksum
    status("complete")


if __name__ == "__main__":
    cmd, *args = sys.argv[1:]
    if cmd == "campaign":
        campaign()
    elif cmd == "handoff":
        handoff(*args)
    else:
        {"fit": fit, "evaluate": evaluate, "transfer": transfer}[cmd](int(args[0]))
