import numpy as np
import ppo
from m6_tune import Vec, gae, to_rates
from make_conditions import TrainingSampler
from sailing import RS, Task, TaskEnv


def test_vec_step_matches_taskenv():
    task = Task.load("U")
    cond = TrainingSampler(5).next()
    vec = Vec(task, 1, 0, conds=[cond])
    ref = TaskEnv(task, cond, 0)
    ref.reset()
    rng = np.random.default_rng(0)
    for _ in range(400):
        a = rng.uniform(-1.2, 1.2, (1, 2))
        _, rew, code = vec.step(a, np.ones(1, bool))
        rec = ref.step(a[0])
        assert np.array_equal(vec.rs[0], rec.next_rs)
        assert rew[0] == rec.reward
        assert code[0] == (rec.outcome != "running")


def test_target_head_reaches_target():
    task = Task.load("U")
    env = TaskEnv(task, TrainingSampler(5).next(), 0)
    env.reset()
    for _ in range(60):  # hauling 3.5 m at 1.5 m/s takes 2.3 s
        env.step(
            to_rates(np.array([0.5, -1.0]), env.research_state(), task.decision_dt)
        )
    rs = env.research_state()
    assert abs(rs[RS["delta_r"]] - 0.349) < 1e-9
    assert abs(rs[RS["l_sheet"]] - 1.0404326023342405) < 1e-6


def test_gae_matches_ppo_at_repeat_1():
    rng = np.random.default_rng(1)
    n, g, lam = 50, 0.99, 0.9
    r, v, vn = rng.normal(size=(3, n))
    boot = np.ones(n)
    cont = np.ones(n)
    boot[10], cont[10] = 0, 0
    cont[30] = 0  # truncation
    ref, _ = ppo.gae(r, v, vn, boot, cont, g, lam)
    disc = np.full(n, g)
    ours = gae(
        r[:, None],
        v[:, None],
        vn[:, None],
        (disc * boot)[:, None],
        disc[:, None],
        cont[:, None],
        lam,
    )
    assert np.allclose(ours[:, 0], ref)


def test_mirror_signs_and_equivariant_nets():
    import jax
    import jax.numpy as jnp
    import sac
    from m5 import StartTask
    from m6_tune import FIXED, MIRROR, make_equivariant
    from sailing import Condition, features

    # U is mirror-symmetric about x = 0 and MIRROR gives the feature signs.
    task = Task.load("U")
    e = TaskEnv(StartTask.at(task, (4.0, 10.0)), Condition("a", 5.1, 3.0, 80.0), 0)
    em = TaskEnv(StartTask.at(task, (-4.0, 10.0)), Condition("b", 5.1, -3.0, 280.0), 0)
    e.reset(), em.reset()
    rng = np.random.default_rng(0)
    for _ in range(300):
        a = rng.uniform(-1, 1, 2)
        r1, r2 = e.step(a), em.step(a * [-1, 1])
        f1, f2 = features(r1.next_rs, task.target), features(r2.next_rs, task.target)
        assert np.allclose(f1 * MIRROR, f2, atol=1e-5)
        assert abs(r1.reward - r2.reward) < 1e-8
    assert all(FIXED["mean"][i] == 0 for i in np.flatnonzero(MIRROR < 0))

    saved = {m: dict(vars(m)) for m in (ppo, sac)}
    try:
        make_equivariant()
        o = jnp.asarray(rng.normal(size=(5, 17)), jnp.float32)
        m = jnp.asarray(MIRROR)
        p = ppo.init(jax.random.PRNGKey(0), 17)
        flip = jnp.array([-1.0, 1.0])
        assert np.allclose(
            ppo.mean_action(p, o * m), flip * ppo.mean_action(p, o), atol=1e-6
        )
        assert np.allclose(ppo.value(p, o * m), ppo.value(p, o), atol=1e-6)
        s = sac.init(jax.random.PRNGKey(1), 17)
        act = jnp.asarray(rng.uniform(-1, 1, (5, 2)), jnp.float32)
        assert np.allclose(
            sac.q_values(s["q"], o * m, act * flip),
            sac.q_values(s["q"], o, act),
            atol=1e-6,
        )
        mu, ls = sac.actor_dist(s["actor"], o)
        mu2, ls2 = sac.actor_dist(s["actor"], o * m)
        assert np.allclose(mu2, flip * mu, atol=1e-6) and np.allclose(
            ls2, ls, atol=1e-6
        )
    finally:
        for mod, d in saved.items():
            vars(mod).update(d)


def test_sac_nstep_transitions():
    """Discounted reward sums, γ^n-weighted bootstrap, the state n steps later;
    a terminal flushes the queue and zeroes every bootstrap after it."""
    import collections

    from m6_tune import nstep_rows

    g, q, out = 0.9, collections.deque(), []
    for t, (r, boot, cont) in enumerate(
        [(1.0, 1, 1), (2.0, 1, 1), (3.0, 1, 1), (4.0, 0, 0)]
    ):
        q.append((t, None, r, g, boot, t + 1))
        out += [(o, R, b, n) for o, _, R, b, n in nstep_rows(q, 2, cont == 0)]
    assert out == [
        (0, 1 + g * 2, g * g, 2),
        (1, 2 + g * 3, g * g, 3),
        (2, 3 + g * 4, 0.0, 4),
        (3, 4.0, 0.0, 4),
    ]
