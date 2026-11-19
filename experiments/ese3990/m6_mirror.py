"""M6 simulator-verified mirror augmentation. Run 'campaign' on the CPU."""

import sys
import time
from collections import deque
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

import m6_campaign
import m6_imitation as imitation
import m6_train
import numpy as np
import ppo
from compare import digest
from m5 import StartTask
from m6 import atomic_json, audit, panel, read, score
from m6_campaign import lines
from m6_width import SEEDS
from make_conditions import held_out_keys
from sailing import Condition, Task, TaskEnv
from viz.episode_store import EpisodeRecorder, EpisodeStore, EpisodeWriter

CONTROL = imitation.ROOT
ROOT = CONTROL.parent / "mirror"
PANELS = ("paired", "previous", "new")
FRESH_SEED = 616201


def reflect(c, xy):
    return replace(
        c,
        id=c.id + "-mirror",
        wind_bearing_deg=-c.wind_bearing_deg,
        heading_deg=(-c.heading_deg) % 360,
    ), [-xy[0], xy[1]]


def attempt(task, source, latent, metadata, writer, exclude):
    """Use the action sequence's actual horizon, stopping on an earlier terminal."""
    m = source.meta
    assert m["kind"] == "training" and m["outcome"] == "finished"
    c, xy = reflect(Condition.from_dict(m["condition"]), m["m5_start_xy"])
    assert c.key() not in exclude
    env = TaskEnv(StartTask.at(task, xy), c, m["seed"])
    rs = env.reset()
    recorder = EpisodeRecorder(env, "float32")
    recorder.start(rs)
    states, rewards = [], []
    mirror_u = latent.copy()
    mirror_u[:, 0] *= -1
    for a, u in zip(source.actions, mirror_u, strict=True):
        reflected = a.copy()
        reflected[0] *= -1
        np.testing.assert_array_equal(np.tanh(u), reflected)
        states.append(rs)
        transition = env.step(reflected)
        recorder.step(transition)
        rewards.append(transition.reward)
        rs = transition.next_rs
        if env.outcome != "running":
            break
    writer.add(
        recorder.finish(
            kind="reflection",
            controller="reflected_training_actions",
            source_condition=m["condition"]["id"],
            source_checksum=m["final_checksum"],
            m5_start_xy=xy,
        )
    )
    if env.outcome != "finished":
        return None
    obs = ppo.normalize(
        np.asarray(states),
        task.target,
        metadata["normalization"],
        metadata["config"]["norm_clip"],
    )
    return obs, mirror_u[: len(obs)], np.asarray(rewards)


def fit(seed):
    task = Task.load("U")
    exclude = held_out_keys()
    pending = deque()
    shadow = ROOT / "reflections" / f"ppo_s{seed}"

    class SourceCollector(m6_train.CurriculumCollector):
        def _log(self):
            if self.env.outcome == "finished":
                assert (
                    self.meta["kind"] == "training" and self.meta["controller"] == "ppo"
                )
                pending.append(
                    (self.recorder.finish(**self.meta), self.curriculum.metadata)
                )
            super()._log()

    class AugmentedBuffer(imitation.SuccessBuffer):
        def collect(self, obs, u, reward, cont, ended, start_decisions):
            first = len(self.wins)
            offset = len(self.rows["u"]) if self.rows is not None else 0
            super().collect(obs, u, reward, cont, ended, start_decisions)
            additions = []
            for win in self.wins[first:]:
                source, meta = pending.popleft()
                length = win["length"]
                assert len(source.actions) == length
                latent = self.rows["u"][offset : offset + length]
                offset += length
                result = attempt(task, source, latent, meta, writer, exclude)
                if result is not None:
                    xs, us, rs = result
                    # The base buffer computes the same terminal return-to-go.
                    extra = BaseBuffer(self.gamma)
                    masks = np.ones(len(rs))
                    masks[-1] = 0
                    extra.collect(xs, us, rs, masks, [{"outcome": "finished"}], 0)
                    additions.append(extra.rows)
            assert not pending
            if additions:
                self.rows = {
                    k: np.concatenate([v, *[r[k] for r in additions]])
                    for k, v in self.rows.items()
                }

    BaseBuffer = imitation.SuccessBuffer
    with (
        EpisodeWriter(shadow / "store", shard_size=8) as writer,
        patch.object(imitation, "ROOT", ROOT),
        patch.object(imitation, "BASE", CONTROL / "pilot"),
        patch.object(imitation, "SuccessBuffer", AugmentedBuffer),
        patch.object(m6_train, "CurriculumCollector", SourceCollector),
    ):
        imitation.fit(seed)
    audit(shadow)
    out = ROOT / "pilot" / f"ppo_s{seed}"
    online = EpisodeStore(out / "store")
    sources = [
        e
        for e in online.index
        if e["kind"] == "training" and e["outcome"] == "finished"
    ]
    mirrors = EpisodeStore(shadow / "store")
    assert len(sources) == len(mirrors.index)
    for a, b in zip(sources, mirrors.index, strict=True):
        assert (a["final_checksum"], a["condition"]["id"]) == (
            b["source_checksum"],
            b["source_condition"],
        )
        expected = online.load(a).actions[: b["decisions"]].copy()
        expected[:, 0] *= -1
        np.testing.assert_array_equal(expected, mirrors.load(b).actions)
    real = sum(e["decisions"] for e in sources)
    added = sum(e["decisions"] for e in mirrors.index if e["outcome"] == "finished")
    updates = lines(out / "imitation.jsonl")
    assert (updates[-1]["samples"] if updates else 0) == real + added
    atomic_json(
        shadow / "cost.json",
        dict(
            real_successes=len(sources),
            real_rows=real,
            mirror_finishes=sum(e["outcome"] == "finished" for e in mirrors.index),
            mirror_rows=added,
            extra_decisions=sum(e["decisions"] for e in mirrors.index),
        ),
    )


def evaluate(seed):
    frozen = read(ROOT / "protocol.json")
    out = ROOT / "pilot" / f"ppo_s{seed}"
    original = CONTROL / "pilot" / f"ppo_s{seed}"
    task = Task.load("U")
    with EpisodeWriter(out / "evaluation/store", shard_size=20) as w:
        for arm, p in [("control", original), ("mirror", out)]:
            path = p / "ckpt/0400000"
            if arm == "control":
                assert all(
                    digest(path.with_suffix(ext)) == frozen["controls"][str(seed)][ext]
                    for ext in (".npz", ".json")
                )
            pol = ppo.PPOPolicy.load(path)
            for name in ("new",) if arm == "control" else (*PANELS, "validation"):
                score(
                    task,
                    w,
                    pol,
                    frozen[name],
                    arm + "_" + name,
                    5 if name == "validation" else 0,
                    400000,
                )
    audit(out / "evaluation")


def campaign():
    ROOT.mkdir(parents=True, exist_ok=False)
    old = read(CONTROL / "protocol.json")
    freeze = read(CONTROL / "freeze.json")
    start = time.time()
    data = dict(
        start=start,
        seeds=SEEDS,
        online_decisions=400000,
        coefficient=imitation.COEFFICIENT,
        paired=old["broad"],
        previous=old["fresh"],
        new=panel(0, FRESH_SEED, 10, 80),
        validation=old["validation"],
        transfer=old["transfer"],
        fresh_seed=FRESH_SEED,
        gate="two of three seeds pass the existing repeated fresh-probe threshold",
        controls={
            s: {
                ext: digest(
                    CONTROL / "pilot" / f"ppo_s{s}" / "ckpt" / ("0400000" + ext)
                )
                for ext in (".npz", ".json")
            }
            for s in SEEDS
        },
        control_freeze=digest(CONTROL / "freeze.json"),
    )
    for name in PANELS:
        assert sum(r["xy"][0] < 0 for r in data[name]) == 40 and len(data[name]) == 80
    atomic_json(ROOT / "protocol.json", data)
    atomic_json(ROOT / "clock.json", {"start": start})
    atomic_json(
        ROOT / "freeze.json",
        {
            **freeze,
            "mirror_protocol": digest(ROOT / "protocol.json"),
            "sources": {
                **freeze["sources"],
                Path(__file__).name: digest(Path(__file__)),
            },
        },
    )

    def jobs(cmd, seeds):
        atomic_json(
            ROOT / "status.json", dict(phase=cmd, elapsed_s=time.time() - start)
        )
        with patch.object(m6_campaign, "ROOT", ROOT):
            m6_campaign.workers(
                [(f"{cmd}_{s}", [Path(__file__).name, cmd, str(s)]) for s in seeds]
            )

    jobs("fit", SEEDS)
    jobs("evaluate", SEEDS)
    passed = [
        s for s in SEEDS if (ROOT / "pilot" / f"ppo_s{s}" / "qualifying.json").exists()
    ]
    if len(passed) >= 2:
        jobs("transfer", passed)
    atomic_json(
        ROOT / "status.json",
        dict(
            phase="complete",
            elapsed_s=time.time() - start,
            qualified=passed,
            transfer=len(passed) >= 2,
        ),
    )


if __name__ == "__main__":
    cmd, *args = sys.argv[1:]
    if cmd == "campaign":
        campaign()
    elif cmd == "transfer":
        with patch.object(imitation, "ROOT", ROOT):
            imitation.transfer(int(args[0]))
    else:
        {"fit": fit, "evaluate": evaluate}[cmd](int(args[0]))
