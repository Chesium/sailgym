"""M6 tuning pilots: vectorized from-scratch PPO/SAC on U (post-hoc, exploratory).

Training-side factors only. Route, dynamics, reward, discount, time limit and
the 20 Hz rate contract sent to Rust are unchanged; every decision still goes
through ``TaskEnv.step``.

* ``--norm calib|fixed``: M4U's calibration statistics (clip 5) or fixed
  physical scales (clip 10). Same 17 features either way.
* ``--head rate|target``: the policy emits the two rates (contract), or a
  rudder-angle / sheet-length target that a dead-beat inner loop turns into
  rates from the *observed* actuator state. The inner loop is part of the
  agent; Rust still receives rates in [-1, 1].
* ``--repeat k``: the policy decides every k decisions and its output is held;
  a learner transition carries the discounted sum of its k rewards and γ^k.
* ``--p-random p``: share of training episodes started at rest at a uniform
  position in ``--box`` with uniform heading (privileged resets, labeled).

Evaluation: the deterministic policy from the original (0, 0) start on 40
fresh training-distribution conditions (``TrainingSampler(90000)``).
Validation, test and shift stay closed.

    env -u PYTHONPATH XLA_FLAGS=--xla_cpu_multi_thread_eigen=false \\
        .venv/bin/python experiments/ese3990/m6_tune.py NAME --seed 0 ...
"""

from __future__ import annotations

import argparse
import collections
import json
import math
import time
from dataclasses import replace

import jax
import jax.numpy as jnp
import numpy as np
import optax
import ppo
import sac
from m5 import StartTask
from make_conditions import TrainingSampler
from probe import powerlaw_noise
from sailing import REPO, RS, Task, TaskEnv, TransitionReward, features

OUT = REPO / "runs/ese3990/M6/tune"
# Actuator limits from crates/sailgym-physics/src/parameters.rs (defaults).
DELTA_MAX, DELTA_RATE = 0.698, 2.09
L_MIN, L_MAX, EASE, HAUL = 1.0404326023342405, 4.5, 3.0, 1.5
FIXED = {
    "mean": [0, 20, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 2.8, 0, 0, -5],
    "std": [
        20,
        20,
        0.7,
        0.7,
        0.2,
        1,
        0.3,
        0.3,
        0.3,
        0.7,
        0.7,
        0.5,
        0.4,
        1,
        1,
        0.5,
        0.5,
    ],
}
CODE = {"running": 0, "finished": 1, "terminated": 2, "truncated": 3}
# Feature signs under reflection across the gate centreline x = 0 (rudder,
# sway, yaw/roll rates, heel, boom and wind_x flip). U is symmetric about it to
# float precision (tests/test_m6_tune.py), and under FIXED (zero mean on every
# flipped feature, symmetric clip) the normalised observation mirrors the same way.
MIRROR = np.array(
    [-1, 1, -1, 1, -1, 1, -1, -1, -1, 1, -1, -1, -1, 1, 1, -1, 1], np.float32
)


def make_equivariant():
    """Patch ppo/sac forwards: π(Ms) = A·π(s), V(Ms) = V(s), Q(Ms, Aa) = Q(s, a).

    Each net f becomes ½(f(s) + B·f(Ms)); losses, updates and checkpoints are
    unchanged. A flips the rudder output only.
    """
    m, a = jnp.asarray(MIRROR), jnp.array([-1.0, 1.0])
    mlp = ppo.mlp

    def mu(p, o):
        return 0.5 * (mlp(p, o) + a * mlp(p, o * m))

    ppo.mean_action = lambda params, o: jnp.tanh(mu(params["actor"], o))
    ppo.value = lambda params, o: (
        0.5 * (mlp(params["critic"], o) + mlp(params["critic"], o * m))[..., 0]
    )
    ppo.log_prob = lambda params, o, u: (
        ppo.gaussian_log_prob(u, mu(params["actor"], o), params["log_std"])
        - ppo.tanh_log_det(u)
    )
    ppo.actor_mean = mu
    b = jnp.array([-1.0, 1.0, 1.0, 1.0])

    def actor_dist(actor, o):
        out = 0.5 * (mlp(actor, o) + b * mlp(actor, o * m))
        mu_, ls = jnp.split(out, 2, axis=-1)
        return mu_, jnp.clip(ls, sac.LOG_STD_MIN, sac.LOG_STD_MAX)

    q_values = sac.q_values

    def q_sym(q, o, act):
        return 0.5 * (q_values(q, o, act) + q_values(q, o * m, act * a))

    sac.actor_dist, sac.q_values = actor_dist, q_sym
    sac.mean_action = lambda params, o: jnp.tanh(actor_dist(params["actor"], o)[0])


def to_rates(a: np.ndarray, rs: np.ndarray, dec_dt: float) -> np.ndarray:
    """Targets in [-1, 1]^2 -> rates that reach them within one decision if
    the rate limit allows (dead-beat)."""
    d, ln = rs[..., RS["delta_r"]], rs[..., RS["l_sheet"]]
    r0 = np.clip((a[..., 0] * DELTA_MAX - d) / (DELTA_RATE * dec_dt), -1, 1)
    r0 = np.where(r0 == 0.0, 1e-9, r0)  # an exact 0 would self-centre the tiller
    diff = L_MIN + (a[..., 1] + 1) * 0.5 * (L_MAX - L_MIN) - ln
    r1 = np.where(diff > 0, diff / (EASE * dec_dt), diff / (HAUL * dec_dt))
    return np.stack([r0, np.clip(r1, -1, 1)], -1)


def upwind_angle(rs: np.ndarray) -> np.ndarray:
    """Signed angle between the heading and the direction the wind comes from."""
    hx, hy = np.cos(rs[..., RS["psi"]]), np.sin(rs[..., RS["psi"]])
    ux, uy = -rs[..., RS["wind_x"]], -rs[..., RS["wind_y"]]
    return np.arctan2(hx * uy - hy * ux, hx * ux + hy * uy)


class Vec:
    """n TaskEnvs with explicit resets; one shared training-condition stream."""

    def __init__(
        self,
        task,
        n,
        seed,
        p_random=0.0,
        box=None,
        conds=None,
        reward=None,
        beta=0.0,
        repeat=1,
    ):
        self.task, self.n, self.p, self.box = task, n, p_random, box
        # Colored exploration noise: one unit-variance 1/f^beta sequence per
        # episode and action dimension, one sample per policy step.
        self.beta = beta
        self.noise_rng = np.random.default_rng([seed, 77])
        self.noise = np.zeros((n, -(-task.max_decisions // repeat) + 1, 2))
        self.nk = np.zeros(n, int)
        self.wind = TrainingSampler(seed)
        self.rng = np.random.default_rng([seed, 66])
        self.conds = conds
        self.count = 0
        self.envs = [None] * n
        self.rs = np.zeros((n, len(RS)))
        self.orig = np.zeros(n, bool)
        self.maxy = np.zeros(n)
        self.tacks = np.zeros(n, int)
        self.ang = np.zeros(n)
        self.reward = TransitionReward(task, reward)
        self.a3 = np.array([0.0, 0.0, -1.0])
        self.o32 = self.f32 = None
        self.buf = np.zeros(len(RS))
        for i in range(n):
            self.reset(i)

    def reset(self, i):
        cond = self.conds[i] if self.conds is not None else self.wind.next()
        task, orig = self.task, True
        if self.p > 0 and self.rng.random() < self.p:
            x0, x1, y0, y1 = self.box
            xy = (float(self.rng.uniform(x0, x1)), float(self.rng.uniform(y0, y1)))
            cond = replace(cond, heading_deg=float(self.rng.uniform(0, 360)))
            task, orig = StartTask.at(self.task, xy), False
        self.envs[i] = TaskEnv(task, cond, self.count)
        self.count += 1
        self.rs[i] = self.envs[i].reset()
        if self.beta > 0:
            L = self.noise.shape[1]
            self.noise[i] = np.stack(
                [powerlaw_noise(self.noise_rng, self.beta, L) for _ in range(2)], 1
            )
            self.nk[i] = 0
        if self.o32 is None:
            self.o32 = np.empty(self.envs[i].spec.obs_len, np.float32)
            self.f32 = np.empty_like(self.o32)
        self.orig[i] = orig
        self.maxy[i] = self.rs[i, RS["y"]]
        self.tacks[i] = 0
        self.ang[i] = upwind_angle(self.rs[i])

    def eps(self):
        """This policy step's colored noise for every env."""
        e = self.noise[np.arange(self.n), self.nk]
        self.nk += 1
        return e

    def step(self, rates, alive):
        """One decision for every alive env: raw Rust step, batched reward.

        Same semantics as ``TaskEnv.step`` (clip to [-1, 1], release -1)."""
        idx = np.flatnonzero(alive)
        prev = self.rs[idx].copy()
        steps = np.zeros(len(idx))
        code = np.zeros(len(idx), int)
        a3 = self.a3
        for j, i in enumerate(idx):
            env = self.envs[i]
            a3[0], a3[1] = (
                min(max(rates[i, 0], -1.0), 1.0),
                min(max(rates[i, 1], -1.0), 1.0),
            )
            _, term, trunc, _, st, _ = env.raw.step(a3, self.o32, self.f32)
            env.raw.research_state(self.buf)
            self.rs[i] = self.buf
            steps[j] = st
            if term or trunc:
                code[j] = CODE[env.outcome]
        nxt = self.rs[idx]
        rew = self.reward.batch(prev, nxt, code, steps)
        self.maxy[idx] = np.maximum(self.maxy[idx], nxt[:, RS["y"]])
        ang = upwind_angle(nxt)
        old = self.ang[idx]
        self.tacks[idx] += (np.abs(ang) < 1.4) & (np.abs(old) < 1.4) & (ang * old < 0)
        self.ang[idx] = ang
        return idx, rew, code

    def record(self, i, code):
        env = self.envs[i]
        return dict(
            orig=bool(self.orig[i]),
            outcome=env.outcome,
            reason=env.raw.termination_reason,
            maxy=float(self.maxy[i]),
            tacks=int(self.tacks[i]),
            t=float(self.rs[i, RS["t"]]),
            dist=math.hypot(self.rs[i, RS["x"]], self.rs[i, RS["y"]] - 40),
        )

    def block(self, a, k, head, gamma, dec_dt):
        """Hold policy output ``a`` (n, 2) for up to k decisions per env."""
        n = self.n
        ret, disc = np.zeros(n), np.ones(n)
        boot, cont = np.ones(n), np.ones(n)
        nrs = np.zeros_like(self.rs)
        ended = []
        alive = np.ones(n, bool)
        executed = 0
        for _ in range(k):
            rates = to_rates(a, self.rs, dec_dt) if head == "target" else a
            idx, rew, code = self.step(rates, alive)
            executed += len(idx)
            ret[idx] += disc[idx] * rew
            disc[idx] *= gamma
            for j in np.flatnonzero(code):
                i = idx[j]
                alive[i] = False
                nrs[i] = self.rs[i]
                boot[i], cont[i] = float(code[j] == CODE["truncated"]), 0.0
                ended.append(self.record(i, code[j]))
        nrs[alive] = self.rs[alive]
        for i in np.flatnonzero(~alive):
            self.reset(i)
        return ret, disc, boot, cont, nrs, ended, executed


def eval_panel(n=40):
    s = TrainingSampler(90000)
    return [s.next() for _ in range(n)]


def evaluate(task, act, args, conds):
    """Deterministic policy from the original start; one env per condition."""
    vec = Vec(task, len(conds), 0, conds=conds)
    alive = np.ones(len(conds), bool)
    out = [None] * len(conds)
    a = np.zeros((len(conds), 2))
    step = 0
    while alive.any():
        if step % args.repeat == 0:
            a = np.asarray(act(vec.rs))
        rates = to_rates(a, vec.rs, task.decision_dt) if args.head == "target" else a
        idx, _, code = vec.step(rates, alive)
        for j in np.flatnonzero(code):
            alive[idx[j]] = False
            out[idx[j]] = vec.record(idx[j], code[j])
        step += 1
    return summarize(out)


def summarize(eps):
    if not eps:
        return {}
    oc = [e["reason"] or e["outcome"] for e in eps]
    fin = [e for e in eps if e["outcome"] == "finished"]
    return dict(
        n=len(eps),
        finished=len(fin),
        **{r: oc.count(r) for r in set(oc) if r != "finished"},
        maxy=float(np.mean([e["maxy"] for e in eps])),
        tack_eps=sum(e["tacks"] > 0 for e in eps),
        **({"t_finish": float(np.mean([e["t"] for e in fin]))} if fin else {}),
        **(
            {"dist": float(np.mean([e["dist"] for e in eps]))}
            if "dist" in eps[0]
            else {}
        ),
    )


class NullWriter:
    def add(self, *_):
        pass


def normalization(task, args):
    if args.norm == "fixed":
        return FIXED, 10.0
    import train

    cfg = {**train.U_CONFIG}
    norm, _ = train.calibrate(
        task, args.seed, cfg, TrainingSampler(args.seed), NullWriter()
    )
    return norm, cfg["norm_clip"]


def kl_update(cfg, target_kl):
    """``ppo.make_update`` with epochs stopped once a minibatch's approx KL
    exceeds 1.5 × target_kl."""
    opt = ppo.make_optimizer(cfg)
    loss_cfg = {k: cfg[k] for k in ("clip", "vf_coef", "ent_coef")}

    @jax.jit
    def step(params, opt_state, batch):
        (_, stats), grads = jax.value_and_grad(ppo.loss, has_aux=True)(
            params, batch, loss_cfg
        )
        updates, opt_state = opt.update(grads, opt_state, params)
        return optax.apply_updates(params, updates), opt_state, stats

    def update(params, opt_state, data, rng):
        n, mb = len(data["adv"]), cfg["minibatch"]
        stats = None
        for _ in range(cfg["epochs"]):
            perm = rng.permutation(n)
            for i in range(0, n - mb + 1, mb):
                batch = {k: v[perm[i : i + mb]] for k, v in data.items()}
                params, opt_state, stats = step(params, opt_state, batch)
            if float(stats["approx_kl"]) > 1.5 * target_kl:
                break
        return params, opt_state, {k: float(v) for k, v in stats.items()}

    return opt, update


def nstep_rows(q, n, done):
    """Pop n-step rows (obs, act, R, bootstrap, next_obs) from one env's queue
    of (obs, act, reward, disc, boot, next_obs); flush everything at an episode end."""
    rows = []
    while q and (done or len(q) == n):
        R, D = 0.0, 1.0
        for _, _, r, d, _, _ in q:
            R += D * r
            D *= d
        rows.append((q[0][0], q[0][1], R, D * q[-1][4], q[-1][5]))
        q.popleft()
    return rows


def gae(ret, v, vn, gb, disc, cont, lam):
    adv = np.zeros_like(ret)
    last = np.zeros(ret.shape[1])
    for t in reversed(range(len(ret))):
        delta = ret[t] + gb[t] * vn[t] - v[t]
        last = delta + lam * disc[t] * cont[t] * last
        adv[t] = last
    return adv


def main(argv=None):
    p = argparse.ArgumentParser()
    p.add_argument("name")
    p.add_argument("--seed", type=int, default=0)
    p.add_argument("--learner", choices=["ppo", "sac"], default="ppo")
    p.add_argument("--norm", choices=["calib", "fixed"], default="fixed")
    p.add_argument("--head", choices=["rate", "target"], default="rate")
    p.add_argument("--repeat", type=int, default=1)
    p.add_argument("--p-random", type=float, default=0.0)
    p.add_argument("--box", type=float, nargs=4, default=[-30, 30, 0, 38])
    p.add_argument("--decisions", type=float, default=10e6)
    p.add_argument("--n-envs", type=int, default=16)
    p.add_argument("--rollout", type=int, default=256, help="policy steps per env")
    p.add_argument("--epochs", type=int, default=4)
    p.add_argument("--minibatch", type=int, default=512)
    p.add_argument("--lam", type=float, default=0.95)
    p.add_argument("--ent", type=float, default=0.001)
    p.add_argument("--log-std", type=float, default=-0.5)
    p.add_argument("--hidden", type=int, default=64)
    p.add_argument("--lr", type=float, default=3e-4)
    p.add_argument("--vf", type=float, default=0.5)
    p.add_argument(
        "--anneal", type=float, default=1.0, help="final lr fraction (linear)"
    )
    p.add_argument("--target-kl", type=float, default=None)
    p.add_argument("--eval-every", type=float, default=1e6)
    # SAC
    p.add_argument("--replay", type=int, default=400_000)
    p.add_argument("--utd", type=float, default=1.0)
    p.add_argument("--warmup", type=int, default=5000, help="random policy steps")
    p.add_argument("--target-entropy", type=float, default=-2.0)
    p.add_argument("--alpha0", type=float, default=0.2)
    p.add_argument("--layernorm", action="store_true")
    p.add_argument(
        "--nstep", type=int, default=1, help="SAC n-step returns, in policy steps"
    )
    p.add_argument("--equivariant", action="store_true")
    p.add_argument(
        "--noise-beta",
        type=float,
        default=0.0,
        help="PPO exploration noise color (1 = pink)",
    )
    p.add_argument(
        "--train-fail", type=float, default=None, help="training-only failure penalty"
    )
    args = p.parse_args(argv)

    out = OUT / f"{args.name}_s{args.seed}"
    out.mkdir(parents=True, exist_ok=False)
    (out / "args.json").write_text(json.dumps(vars(args), indent=1))
    task = Task.load("U")
    gamma = task.reward["gamma"]
    dec_dt = task.decision_dt
    norm, clip = normalization(task, args)
    decisions = 10_000 if args.norm == "calib" else 0
    mean, std = np.asarray(norm["mean"]), np.asarray(norm["std"])

    def obs(rs):
        z = (features(rs, task.target) - mean) / std
        return np.clip(z, -clip, clip).astype(np.float32)

    key = jax.random.PRNGKey(args.seed)
    rng = np.random.default_rng([args.seed, 1])
    if args.equivariant:
        assert args.norm == "fixed", "mirroring needs the zero-mean fixed scales"
        make_equivariant()
    train_reward = (
        None
        if args.train_fail is None
        else {**task.reward, "failure_penalty": args.train_fail}
    )
    vec = Vec(
        task,
        args.n_envs,
        args.seed,
        args.p_random,
        args.box,
        reward=train_reward,
        beta=args.noise_beta,
        repeat=args.repeat,
    )
    panel = eval_panel()
    k = args.repeat
    t0 = time.monotonic()
    episodes = []
    next_eval = 0.0

    if args.learner == "ppo":
        params = ppo.init(key, 17, 2, args.hidden, args.log_std)
        cfg = dict(
            clip=0.2,
            vf_coef=args.vf,
            ent_coef=args.ent,
            lr=args.lr,
            max_grad_norm=1.0,
            minibatch=args.minibatch,
            epochs=args.epochs,
        )
        if args.anneal < 1:
            per_update = args.epochs * (args.rollout * args.n_envs // args.minibatch)
            total = (
                int(args.decisions / (args.rollout * args.n_envs * k) + 1) * per_update
            )
            cfg["lr"] = optax.linear_schedule(args.lr, args.lr * args.anneal, total)
        opt, update = (
            ppo.make_update(cfg)
            if args.target_kl is None
            else kl_update(cfg, args.target_kl)
        )
        opt_state = opt.init(params)
        actor = jax.jit(
            lambda p, o: getattr(ppo, "actor_mean", lambda a, x: ppo.mlp(a, x))(
                p["actor"], o
            )
        )
        value, logp_fn = jax.jit(ppo.value), jax.jit(ppo.log_prob)
        mean_act = jax.jit(ppo.mean_action)
    else:
        params = sac.init(key, 17, 2, args.hidden, args.alpha0, args.layernorm)
        scfg = dict(
            lr=args.lr, max_grad_norm=1.0, tau=0.005, target_entropy=args.target_entropy
        )
        init_opt, sac_update = sac.make_update(scfg, 1.0)  # γ^n folded into bootstrap
        scan_update = jax.jit(lambda st, b: jax.lax.scan(sac_update, st, b))
        state = dict(
            params=params,
            target_q=params["q"],
            opt=init_opt(params),
            key=jax.random.fold_in(key, 1),
        )
        replay = sac.Replay(args.replay, 17)
        dist = jax.jit(sac.actor_dist)
        mean_act = jax.jit(sac.mean_action)
        policy_steps = 0
        update_debt = 0.0
        nq = [collections.deque() for _ in range(args.n_envs)]

    def act_det(rs):
        return mean_act(params, obs(rs))

    updates = 0
    last_stats = {}
    while True:
        if decisions >= next_eval:
            ts = time.monotonic()
            ev = evaluate(task, act_det, args, panel)
            row = dict(
                decisions=int(decisions),
                wall=round(time.monotonic() - t0, 1),
                eval_s=round(time.monotonic() - ts, 1),
                updates=updates,
                eval=ev,
                train=summarize(episodes),
                train_orig=summarize([e for e in episodes if e["orig"]]),
            )
            if args.learner == "ppo":
                row["log_std"] = np.asarray(params["log_std"]).round(3).tolist()
            else:
                row.update({k_: float(v_) for k_, v_ in last_stats.items()})
            with open(out / "log.jsonl", "a") as log:
                log.write(json.dumps(row) + "\n")
            print(json.dumps(row), flush=True)
            np.savez(
                out / f"ckpt_{int(decisions):09d}.npz",
                *[np.asarray(x) for x in jax.tree.leaves(params)],
            )
            episodes = []
            next_eval += args.eval_every
        if decisions >= args.decisions:
            break
        if args.learner == "ppo":
            std_ = np.exp(np.asarray(params["log_std"]))
            T, N = args.rollout, args.n_envs
            buf = {x: np.zeros((T, N)) for x in ("ret", "disc", "gb", "cont")}
            OB = np.zeros((T, N, 17), np.float32)
            ON = np.zeros((T, N, 17), np.float32)
            U = np.zeros((T, N, 2), np.float32)
            for t in range(T):
                o = obs(vec.rs)
                eps = vec.eps() if args.noise_beta > 0 else rng.standard_normal((N, 2))
                u = (np.asarray(actor(params, o)) + std_ * eps).astype(np.float32)
                ret, disc, boot, cont, nrs, ended, ex = vec.block(
                    np.tanh(u), k, args.head, gamma, dec_dt
                )
                decisions += ex
                episodes += ended
                OB[t], ON[t], U[t] = o, obs(nrs), u
                buf["ret"][t], buf["disc"][t], buf["gb"][t], buf["cont"][t] = (
                    ret,
                    disc,
                    disc * boot,
                    cont,
                )
            flat = OB.reshape(-1, 17)
            v = np.asarray(value(params, flat)).reshape(T, N)
            vn = np.asarray(value(params, ON.reshape(-1, 17))).reshape(T, N)
            adv = gae(buf["ret"], v, vn, buf["gb"], buf["disc"], buf["cont"], args.lam)
            batch = dict(
                obs=flat,
                u=U.reshape(-1, 2),
                logp=np.asarray(logp_fn(params, flat, U.reshape(-1, 2))),
                adv=adv.reshape(-1).astype(np.float32),
                ret=(adv + v).reshape(-1).astype(np.float32),
            )
            params, opt_state, stats = update(params, opt_state, batch, rng)
            updates += 1
        else:
            N = args.n_envs
            o = obs(vec.rs)
            if policy_steps * N < args.warmup:
                a = rng.uniform(-1, 1, (N, 2))
            else:
                mu, ls = map(np.asarray, dist(params["actor"], o))
                eps = vec.eps() if args.noise_beta > 0 else rng.standard_normal((N, 2))
                a = np.tanh(mu + np.exp(ls) * eps)
            ret, disc, boot, cont, nrs, ended, ex = vec.block(
                a, k, args.head, gamma, dec_dt
            )
            decisions += ex
            episodes += ended
            policy_steps += 1
            nobs = obs(nrs)
            rows = []
            for i in range(N):  # n-step transitions; nstep = 1 is the ordinary one
                nq[i].append((o[i], a[i], ret[i], disc[i], boot[i], nobs[i]))
                rows += nstep_rows(nq[i], args.nstep, cont[i] == 0)
            if rows:
                o_, a_, r_, b_, n_ = map(np.asarray, zip(*rows, strict=True))
                replay.add(
                    obs=o_,
                    act=a_.astype(np.float32),
                    reward=r_,
                    next_obs=n_,
                    bootstrap=b_,
                    cont=np.ones(len(r_)),
                )
            if policy_steps * N >= args.warmup:
                update_debt += N * args.utd
                u_n = int(update_debt)
                if u_n:
                    idx = rng.integers(0, replay.size, (u_n, 256))
                    batches = {k_: v_[idx] for k_, v_ in replay.data.items()}
                    state, st = scan_update(state, batches)
                    last_stats = {k_: v_[-1] for k_, v_ in st.items()}
                    update_debt -= u_n
                    updates += u_n
                params = state["params"]


if __name__ == "__main__":
    main()
