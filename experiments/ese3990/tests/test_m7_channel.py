import m6_tune
import m7_channel
import numpy as np
import pytest
from m7_channel_mpc import channel
from make_conditions import TrainingSampler
from sailing import Task, TaskEnv


def test_channel_only_narrows_x():
    u, c = Task.load("U"), channel(10.0)
    assert c.bounds == (-10.0, u.bounds[1], 10.0, u.bounds[3])
    assert (c.target, c.start, c.budget_s, c.reward) == (
        u.target,
        u.start,
        u.budget_s,
        u.reward,
    )


@pytest.fixture
def stub_eval(monkeypatch):
    """m6_tune.evaluate replaced by panel scores keyed on the task's max x."""

    def install(scores):
        monkeypatch.setattr(
            m6_tune,
            "evaluate",
            lambda task, act, args, conds: {"finished": scores[task.bounds[2]]},
        )

    return install


def run(task, n):
    return [m6_tune.evaluate(task, None, None, None)["curriculum"] for _ in range(n)]


def test_curriculum_shrinks_and_backs_off(stub_eval):
    stub_eval({15.0: 40, 12.5: 40, 10.0: 0})
    m7_channel.width_curriculum(10.0, 2.5, 30, 15.0, backoff=2)
    hs = [c["half"] for c in run(channel(15.0), 9)]
    assert hs == [15, 12.5, 10, 10, 12.5, 10, 10, 12.5, 10]


def test_curriculum_open_first_then_start(stub_eval):
    u = Task.load("U")
    stub_eval({u.bounds[2]: 40, 30.0: 0, 10.0: 0})
    m7_channel.width_curriculum(10.0, 2.5, 30, 30.0, backoff=2, first=u.bounds[2])
    task = Task.load("U")
    cur = run(task, 4)
    # U's own box is evaluated as-is, then ±30; back-off never returns to U
    assert [c["half"] for c in cur] == [u.bounds[2], 30, 30, 30]
    assert task.bounds == (-30.0, u.bounds[1], 30.0, u.bounds[3])


def test_mix_task_draws_width_per_episode():
    t = m7_channel.mix_task(10.0, 30.0, seed=0)
    cond = TrainingSampler(5).next()
    bs = [TaskEnv(t, cond, 0).spec.bounds for _ in range(20)]
    hs = [b[2] for b in bs]
    assert all(10.0 <= h <= 30.0 for h in hs) and len(set(hs)) == 20
    assert np.allclose([b[0] for b in bs], [-h for h in hs])  # symmetric box
