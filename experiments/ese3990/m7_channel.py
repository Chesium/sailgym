"""M7 channel beat: U with the x bounds narrowed to ±HALF m (route, start,
reward, γ, budget and the 20 Hz rates sent to Rust unchanged).

Runs the M6 tuning trainer, evaluator and held-out driver unchanged, with
their ``Task.load("U")`` swapped for the channel task and outputs under
runs/ese3990/M7/train. Each run records its width in ``channel.json``.

  m7_channel.py train NAME --seed S [--half 10] [--curriculum START STEP NEED]
      [--backoff N] [--open-first W] [--mix-lo LO] [m6_tune args...]
  m7_channel.py eval RUN CKPT SPLIT [--jobs N]
  m7_channel.py report [PREFIX ...]
  m7_channel.py heldout OUT.json RUN... [--budget B] [--workers W]
  m7_channel.py results OUT.json
"""

import argparse
import copy
import json
import subprocess
import sys
from dataclasses import dataclass, fields
from pathlib import Path

import m6_tune
import m6_tune_eval
import m6_tune_heldout
import m6_tune_plot
import numpy as np
import u_diagnostics
from m7_channel_mpc import channel
from sailing import REPO, Task
from viz.episode_store import EpisodeStore, EpisodeWriter
from viz.replay import replay

OUT = REPO / "runs/ese3990/M7/train"


@dataclass
class MixTask(Task):
    """Training only: each episode's half-width is uniform in [lo, hi]."""

    lo: float = 10.0
    hi: float = 30.0
    seed: int = 0

    def __post_init__(self):
        self.rng = np.random.default_rng([self.seed, 88])

    def spec(self, cond):  # called once per TaskEnv, i.e. per episode
        t, h = copy.copy(self), float(self.rng.uniform(self.lo, self.hi))
        _, y0, _, y1 = self.bounds
        t.bounds = (-h, y0, h, y1)
        return Task.spec(t, cond)


def mix_task(lo, hi, seed):
    base = channel(hi)
    return MixTask(
        **{f.name: getattr(base, f.name) for f in fields(Task)}, lo=lo, hi=hi, seed=seed
    )


class _ChannelLoader:
    def __init__(self, make, out=None, meta=None):
        self.make, self.out, self.meta = make, out, meta

    def load(self, name):
        assert name == "U", name
        if self.out is not None:  # m6_tune.main has just created the run dir
            (self.out / "channel.json").write_text(json.dumps(self.meta))
        return self.make()


def use_channel(half, out=None, meta=None, make=None):
    make = make or (lambda: channel(half))
    for m in (m6_tune, m6_tune_eval, m6_tune_plot):
        m.Task = _ChannelLoader(make, out, meta or {"half": half})


def width_curriculum(final, step, need, start, backoff=None, first=None):
    """Shrink the training channel by ``step`` m each time the deterministic
    panel at the current width finishes >= ``need``/40. With ``backoff``,
    widen one step (up to ``start``) after that many consecutive 0/40 panels
    at a shrunk width. With ``first``, train at ±first m until the panel
    there reaches ``need``, then jump to ``start``. The logged ``eval``
    (checkpoint selection) is always at the final width."""
    orig, final_task = m6_tune.evaluate, channel(final)
    zeros = [0]

    def set_half(task, h):
        if isinstance(task, MixTask):
            task.hi = h
        else:
            _, y0, _, y1 = task.bounds
            task.bounds = (-h, y0, h, y1)  # Vec resets read task.bounds

    def evaluate(task, act, args, conds):
        mix = isinstance(task, MixTask)
        half = task.hi if mix else task.bounds[2]
        cur = orig(channel(half) if mix else task, act, args, conds)
        res = orig(final_task, act, args, conds) if half != final else dict(cur)
        k = cur.get("finished", 0)
        res["curriculum"] = {"half": half, "finished": k}
        zeros[0] = zeros[0] + 1 if k == 0 and half < start else 0
        if half == first:
            if k >= need:
                set_half(task, start)
        elif k >= need and half > final:
            set_half(task, max(final, half - step))
        elif backoff and zeros[0] >= backoff:
            set_half(task, min(start, half + step))
            zeros[0] = 0
        return res

    m6_tune.evaluate = evaluate


def _diagnose_store(store, jobs=6, cfg=None, chunk=8):
    """u_diagnostics.diagnose_store in-process: its spawned workers would
    reload open-water U and fail the exact replay of channel episodes."""
    cfg = cfg or u_diagnostics.load_config()
    st, task = EpisodeStore(Path(store)), m6_tune_eval.Task.load("U")
    return {
        e["episode_id"]: u_diagnostics.diagnose(
            replay(st.load(e), st.identity, task), task, cfg
        )
        for e in st.index
    }


def half_of(run):
    return json.loads((Path(run) / "channel.json").read_text())["half"]


class _Sub:
    """Route m6_tune_heldout's evaluator subprocess through this module."""

    @staticmethod
    def run(cmd, **kw):
        assert cmd[1] == "m6_tune_eval.py", cmd
        return subprocess.run([cmd[0], Path(__file__).name, "eval", *cmd[2:]], **kw)


def main():
    cmd, rest = sys.argv[1], sys.argv[2:]
    if cmd == "train":
        p = argparse.ArgumentParser()
        p.add_argument("name")
        p.add_argument("--seed", type=int, default=0)
        p.add_argument("--half", type=float, default=10.0)
        p.add_argument(
            "--curriculum",
            type=float,
            nargs=3,
            metavar=("START", "STEP", "NEED"),
            help="train from ±START m, shrinking by STEP m at NEED/40 panel finishes"
            " (NEED 41 never shrinks)",
        )
        p.add_argument(
            "--backoff",
            type=int,
            help="widen one step after this many consecutive 0/40 shrunk panels",
        )
        p.add_argument(
            "--open-first",
            type=float,
            metavar="W",
            help="train at ±W m until NEED/40 there, then start the curriculum",
        )
        p.add_argument(
            "--open-first-u",
            action="store_true",
            help="as --open-first, with the frozen U bounds themselves: training is"
            " identical to m6_tune's until the first switch",
        )
        p.add_argument(
            "--mix-lo",
            type=float,
            help="per-episode training half-width uniform in [MIX_LO, current width]",
        )
        a, other = p.parse_known_args(rest)
        meta = {
            "half": a.half,
            "curriculum": a.curriculum,
            "mix_lo": a.mix_lo,
            "backoff": a.backoff,
            "open_first": a.open_first,
            "open_first_u": a.open_first_u,
        }
        start, make = a.half, None
        if a.open_first_u:
            assert a.curriculum and not a.open_first and a.mix_lo is None
            a.open_first = Task.load("U").bounds[2]  # max x of the asymmetric U box
            make = lambda: Task.load("U")  # noqa: E731
        if a.curriculum:
            start, step, need = a.curriculum
            width_curriculum(a.half, step, need, start, a.backoff, a.open_first)
            start = a.open_first or start
        if a.mix_lo is not None:
            assert a.curriculum, "--mix-lo needs --curriculum (eval at the final width)"
            make = lambda: mix_task(a.mix_lo, start, a.seed)  # noqa: E731
        use_channel(start, OUT / f"{a.name}_s{a.seed}", meta, make)
        m6_tune.OUT = OUT
        m6_tune.main([a.name, "--seed", str(a.seed), *other])
    elif cmd == "eval":
        half = half_of(rest[0])
        use_channel(half)
        u_diagnostics.diagnose_store = _diagnose_store
        # stores say task "U"; record the channel so they are not replayed as U
        m6_tune_eval.EpisodeWriter = lambda path, run: EpisodeWriter(
            path, {**run, "m7_channel_half_m": half}
        )
        sys.argv = ["m6_tune_eval.py", *rest]
        m6_tune_eval.main()
    elif cmd == "report":  # finish curves per run, as m6_tune_report.py
        m6_tune.OUT = OUT
        sys.argv = ["m6_tune_report.py", *rest]
        import m6_tune_report  # noqa: F401  (a script: runs on import)
    elif cmd == "heldout":
        m6_tune_heldout.OUT = OUT
        m6_tune_heldout.subprocess = _Sub
        sys.argv = ["m6_tune_heldout.py", *rest]
        m6_tune_heldout.main()
    elif cmd == "results":  # m6_tune_results.py over M7, plus each run's channel
        m6_tune.OUT = OUT
        import m6_tune_results

        m6_tune_results.main(rest[0])
        doc = json.loads(Path(rest[0]).read_text())
        doc["label"] = (
            "M7 channel-beat pilots (post-hoc, exploratory); U with x bounds ±10 m; "
            "panel_finishes are the 40 training-distribution conditions "
            "(TrainingSampler(90000)) at ±10 m; train_width/_finishes are the "
            "curriculum's current width and its panel there"
        )
        for name, r in doc["runs"].items():
            rows = [json.loads(x) for x in (OUT / name / "log.jsonl").open()]
            cur = [r_["eval"].get("curriculum") or {} for r_ in rows]
            r["channel"] = json.loads((OUT / name / "channel.json").read_text())
            r["train_width"] = [c.get("half") for c in cur]
            r["train_width_finishes"] = [c.get("finished") for c in cur]
        Path(rest[0]).write_text(json.dumps(doc, indent=1))
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
