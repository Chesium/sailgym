"""Planning branches through the Python binding (plan §4.3, E0).

The Rust suite `crates/sailgym-env/tests/fork.rs` proves the boundary cases;
this checks the same property end to end through `sailgym_core`, with the
release-restricted RuleSailor driving the live prefix.
"""

from __future__ import annotations

import numpy as np
import pytest
from baselines import RuleSailorController
from sailing import NOMINAL, Task, TaskEnv, load_manifest


def drive(raw, actions, obs_len):
    o = np.empty(obs_len, np.float32)
    f = o.copy()
    rs = np.empty(20)
    out = []
    for a in actions:
        if raw.outcome != "running":
            break
        r = raw.step(np.asarray(a, dtype=np.float64), o, f)
        raw.research_state(rs)
        out.append((r, rs.copy(), raw.outcome, raw.steps))
    return out


def rule_prefix(task, cond, n):
    env = TaskEnv(task, cond)
    env.reset()
    ctl = RuleSailorController()
    ctl.reset(env)
    acts = []
    for _ in range(n):
        a = env.adapter(ctl.act(env)).copy()
        env.step(a)
        acts.append(a)
    return env, acts


def assert_same(a, b):
    assert len(a) == len(b)
    for (ra, sa, oa, na), (rb, sb, ob, nb) in zip(a, b, strict=True):
        assert ra == rb and oa == ob and na == nb
        np.testing.assert_array_equal(sa, sb)


def test_branch_equals_reset_replay_and_leaves_live_untouched():
    task = Task.load("R")
    for cond in (NOMINAL, load_manifest("validation")[3]):
        live, prefix = rule_prefix(task, cond, 150)
        before = live.research_state()
        rng = np.random.default_rng(1)
        future = [np.r_[rng.uniform(-1, 1, 2), -1.0] for _ in range(400)]

        b1 = live.fork()
        got = drive(b1, future, live.spec.obs_len)
        np.testing.assert_array_equal(live.research_state(), before)

        ref = TaskEnv(task, cond)
        ref.reset()
        for a in prefix:
            ref.step(a)
        want = drive(ref.raw, future, ref.spec.obs_len)
        assert_same(got, want)

        # Evaluating other candidates first changes nothing.
        for _ in range(3):
            drive(
                live.fork(), [np.r_[rng.uniform(-1, 1, 2), -1.0] for _ in range(50)], 18
            )
        assert_same(drive(live.fork(), future, live.spec.obs_len), want)
        np.testing.assert_array_equal(live.research_state(), before)


def test_a_terminal_branch_never_resets():
    """A branch stops where its episode ends and refuses to step further."""
    task = Task.load("R")
    live, _ = rule_prefix(task, NOMINAL, 250)  # ~12.5 s; the gate is at ~14.8 s
    assert live.outcome == "running"
    branch = live.fork()
    o = np.empty(live.spec.obs_len, np.float32)
    f = o.copy()
    while branch.outcome == "running":
        branch.step(np.array([0.0, -0.2, -1.0]), o, f)
    with pytest.raises(ValueError):
        branch.step(np.array([0.0, 0.0, -1.0]), o, f)
    assert live.outcome == "running"


def test_native_batch_rollout_equals_fork_stepping_and_leaves_live_untouched():
    """`TaskEnv.rollout` (Rust `RawEnv.rollout`) against one fork per candidate
    driven from Python, through the budget boundary (plan §4.3)."""
    task = Task.load("R")
    live = TaskEnv(task, NOMINAL)
    live.reset()
    # Circle with the rudder hard over until H runs past the time limit.
    while live.raw.steps < task.max_steps - 6 * task.cadence:
        live.step(np.array([1.0, 1.0]))
    assert live.outcome == "running"
    before, steps_before = live.research_state(), live.raw.steps
    rng = np.random.default_rng(2)
    rates = rng.uniform(-1.5, 1.5, size=(4, 10, 2))  # out-of-range rates get clipped
    nrs, code, steps, rew = live.rollout(rates)
    np.testing.assert_array_equal(live.research_state(), before)
    assert live.raw.steps == steps_before

    for k in range(4):
        acts = [live.adapter(a).copy() for a in rates[k]]
        want = drive(live.fork(), acts, live.spec.obs_len)
        n = len(want)
        assert n == 6  # truncated on the 6th decision
        cum = steps_before + np.cumsum(steps[k, :n])
        np.testing.assert_array_equal(cum, [w[3] for w in want])
        assert (steps[k, n:] == 0).all() and (rew[k, n:] == 0).all()
        assert code[k, n - 1] == 3 and (code[k, : n - 1] == 0).all()
        for i, (_, s, _, _) in enumerate(want):
            np.testing.assert_array_equal(nrs[k, i], s)
        prev = before
        for i in range(n):
            o = {0: "running", 3: "truncated"}[code[k, i]]
            assert rew[k, i] == live.reward_fn(prev, nrs[k, i], o, int(steps[k, i]))
            prev = nrs[k, i]
