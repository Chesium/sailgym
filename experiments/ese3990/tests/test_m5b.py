import jax
import jax.numpy as jnp
import m5b
import numpy as np
import ppo
import sac
from probe import powerlaw_noise
from sailing import Task
from train import Collector, TrainingSampler


def test_critic_target_is_zero_entropy_soft_target():
    key = jax.random.PRNGKey(0)
    params = sac.init(key, 5)
    batch = dict(
        next_obs=np.ones((2, 5), np.float32),
        reward=np.array([2.0, 3.0]),
        bootstrap=np.array([0.0, 1.0]),
    )
    expected = sac.soft_target(
        dict(params, log_alpha=jnp.array(-jnp.inf)), params["q"], batch, key, 0.99975
    )
    np.testing.assert_array_equal(
        m5b.task_target(params, params["q"], batch, key, 0.99975), expected
    )


def test_pink_realization():
    rng = np.random.default_rng(12)
    expected = np.stack([powerlaw_noise(rng, 1, 256) for _ in range(2)], axis=1)
    np.testing.assert_array_equal(
        m5b.pink_sequence(np.random.default_rng(12), 256), expected
    )


def test_eight_conditions_and_independent_advantages():
    sampler = TrainingSampler(0)
    task = Task.load("U")
    lanes = [Collector(task, sampler, None, {}, "float32") for _ in range(8)]
    assert len({lane.env.cond.id for lane in lanes}) == 8
    batch, _ = m5b.EightCollectors(lanes).run(2048, lambda _: (np.zeros(2), 0))
    assert batch["rs"].shape[0] == 2048
    assert np.all(batch["cont"][255::256] == 0)
    reward = np.repeat(np.arange(1, 9), 256)
    zeros = np.zeros(2048)
    adv, _ = ppo.gae(
        reward, zeros, zeros, batch["bootstrap"], batch["cont"], 0.99, 0.95
    )
    np.testing.assert_array_equal(adv[255::256], np.arange(1, 9))
