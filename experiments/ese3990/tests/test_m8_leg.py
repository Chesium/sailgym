import math
from dataclasses import astuple

import m8_leg as m8
import numpy as np
from baselines import RuleSailorController
from sailing import RS, Task, TaskEnv, load_manifest

NRS = m8.NRS


def det_policy(seed=0):
    """A deterministic function of the leg-frame features (no RNG state)."""
    w = np.random.default_rng(seed).normal(size=(m8.OBS, 2))
    return lambda rs: np.tanh(m8.leg_obs(rs) @ w)


def test_leg_reproduces_u_rewards_and_outcome():
    u, cond = Task.load("U"), load_manifest("test")[0]
    leg = m8.Leg(
        "t", cond.wind_speed, cond.wind_bearing_deg, 40.0, None, cond.heading_deg, 0.0
    )
    task = m8.leg_task(leg, bounds=u.bounds, decisions=u.max_decisions)
    assert (task.target, task.max_steps, task.route_json) == (
        u.target,
        u.max_steps,
        u.route_json,
    )
    ref = TaskEnv(u, cond, 0)
    ref.reset()
    vec = m8.LegVec(1, legs=[leg], task_fn=lambda _: task)
    rng = np.random.default_rng(0)
    for t in range(u.max_decisions):
        if t % 10 == 0:
            a = rng.uniform(-1, 1, 2)
        rec = ref.step(a)
        _, rew, code = vec.step(a[None], np.ones(1, bool))
        assert rew[0] == rec.reward
        assert np.array_equal(vec.rs[0, :NRS], rec.next_rs)
        if rec.outcome != "running":
            assert code[0] == m8.CODE[rec.outcome]
            break
    assert rec.outcome != "running"


def test_features_invariant_when_course_and_wind_rotate_together():
    leg = m8.Leg("r", 5.0, 40.0, 60.0, None, 100.0, 1.0)
    act = det_policy()
    for th in (0.0, 37.0, 135.0, -100.0):
        b = (leg.D * math.cos(math.radians(th)), leg.D * math.sin(math.radians(th)))
        rot = 90.0 - th  # compass offset of the rotated leg
        route = m8.RouteEp(
            "rot", (b,), None, leg.wind_speed, (leg.wind_from_deg + rot) % 360,
            (leg.heading_deg + rot) % 360, leg.speed,
        )  # fmt: skip
        lv = m8.LegVec(1, legs=[leg])
        rv = m8.RouteVec(1, legs=[route])
        # rounding differences start at 1e-16 and grow about 1000x per 100
        # decisions under this sharp policy (measured): 1e-12 by 200, 1e-9 by 300
        for _ in range(250):
            a = act(lv.rs)
            np.testing.assert_allclose(
                m8.leg_features(rv.rs), m8.leg_features(lv.rs), atol=1e-6
            )
            rates = m8.to_rates(a, lv.rs, m8.U.decision_dt)
            lv.step(rates, np.ones(1, bool))
            rv.step(m8.to_rates(act(rv.rs), rv.rs, m8.U.decision_dt), np.ones(1, bool))


def test_one_leg_route_equals_the_single_leg_episode():
    legs = [
        m8.Leg("a", 5.2, 170.0, 30.0, None, 270.0, 0.5),
        m8.Leg("b", 4.8, -60.0, 30.0, None, 90.0, 0.0),
    ]
    routes = [
        m8.RouteEp(g.id, ((0.0, g.D),), None, *astuple(g)[1:3], g.heading_deg, g.speed)
        for g in legs
    ]
    a, _ = m8.run_legs(legs, det_policy(1), record=True)
    b, _ = m8.run_legs(routes, det_policy(1), record=True, task_fn=m8.route_task)
    for x, y in zip(a, b, strict=True):
        assert y.pop("legs") == 1 and y.pop("legs_passed") == (
            x["outcome"] == "finished"
        )
        for r in (x, y):  # routes are labelled by name, legs by bearing bin
            del r["bin"], r["cell"]
        assert x == y


def test_stored_episodes_replay_exactly():
    legs = m8.load_split("validation")[::12]
    recs, vec = m8.run_legs(legs, det_policy(2), record=True)
    for i, (leg, r) in enumerate(zip(legs, recs, strict=True)):
        h, oc, t = m8.replay_leg(leg, np.asarray(vec.rec[i]), r["env_seed"])
        assert (h, oc, t) == (r["state_sha256"], r["outcome"], r["t"])
    # a perturbed action changes the hash
    acts = np.asarray(vec.rec[0]).copy()
    acts[5, 0] = -acts[5, 0] if acts[5, 0] else 0.5
    assert (
        m8.replay_leg(legs[0], acts, recs[0]["env_seed"])[0] != recs[0]["state_sha256"]
    )


def test_wrapper_follows_the_rust_tracker_round_a_route():
    """RuleSailor sails the triangle; Rust finishes it only after every gate,
    and the wrapper's leg index rose 0 -> 1 -> 2 in order on the way.
    (guidance.distance_to_target is a lookahead point, not the mark.)"""
    cond = load_manifest("test")[0]
    wps = m8.routes()["triangle"]["waypoints"]
    r = m8.RouteEp("triangle", tuple(map(tuple, wps)), None, *astuple(cond)[1:])
    vec = m8.RouteVec(1, legs=[r])
    c = RuleSailorController()
    c.reset(vec.envs[0])
    rates, ks = np.zeros((1, 2)), [0]
    for _ in range(vec.envs[0].task.max_decisions):
        rates[0] = c.act(vec.envs[0])[:2]
        _, _, code = vec.step(rates, np.ones(1, bool))
        if code[0]:
            break
        if vec.leg_k[0] != ks[-1]:
            ks.append(int(vec.leg_k[0]))
            rs = vec.rs[
                0
            ]  # just past the previous gate: the new leg starts at its mark
            assert math.hypot(rs[RS["x"]], rs[RS["y"]]) < GATE + 2.0
    rec = vec.record(0, code[0])
    assert ks == [0, 1, 2]
    assert rec["outcome"] == "finished" and rec["legs_passed"] == rec["legs"] == 3


GATE = m8.GATE_HALF
