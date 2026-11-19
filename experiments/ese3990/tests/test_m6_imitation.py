import jax
import numpy as np
import ppo
from m6_imitation import Handoff, SuccessBuffer, imitation_loss


def test_handoff_switches_after_40_live_decisions_and_resets():
    class Policy:
        def __init__(self, value):
            self.value = value

        def reset(self, env):
            pass

        def act(self, env):
            return self.value + env

    p = Handoff(Policy(10), Policy(20), 40)
    for _ in range(2):
        p.reset(0)
        assert [p.act(i) for i in range(42)] == [
            i + (10 if i < 40 else 20) for i in range(42)
        ]


def test_buffer_keeps_full_finished_training_episode_across_rollouts():
    buf = SuccessBuffer(0.5)

    def collect(start, values, cont, outcomes):
        xs = np.array(values)
        buf.collect(
            xs[:, None],
            (-xs)[:, None],
            xs,
            np.array(cont),
            [{"outcome": o} for o in outcomes],
            start,
        )

    collect(100, [99, 1], [0, 1], ["truncated"])
    assert buf.rows is None
    collect(
        102, [2, 4, 80, 90, 7], [1, 0, 0, 0, 1], ["finished", "terminated", "truncated"]
    )
    np.testing.assert_array_equal(buf.rows["obs"][:, 0], [1, 2, 4])
    np.testing.assert_array_equal(buf.rows["returns"], [3, 4, 4])
    assert buf.wins == [{"decisions": 104, "length": 3}]
    collect(107, [8], [0], ["finished"])
    np.testing.assert_array_equal(buf.rows["u"][:, 0], [-1, -2, -4, -7, -8])
    np.testing.assert_array_equal(buf.rows["returns"], [3, 4, 4, 11, 8])


def test_advantage_is_positive_only_and_value_has_no_imitation_gradient():
    p = ppo.init(jax.random.PRNGKey(0), 3, 2)
    batch = dict(obs=np.ones((4, 3)), u=np.ones((4, 2)))

    def gradients(returns):
        return jax.grad(
            lambda pars: imitation_loss(pars, {**batch, "returns": returns})[0]
        )(p)

    negative = gradients(np.full(4, -100))
    assert all(np.all(np.asarray(v) == 0) for v in jax.tree.leaves(negative))
    positive = gradients(np.full(4, 100))
    assert any(np.any(np.asarray(v) != 0) for v in jax.tree.leaves(positive["actor"]))
    assert all(np.all(np.asarray(v) == 0) for v in jax.tree.leaves(positive["critic"]))
