"""M4X (plan §6): post-hoc, exploratory arms that test two explanations of
M4U's upwind results. Every M4X row is labeled exploratory and never merged
into an M4/M4U table; no M4/M4U freeze, row or store is touched.

Usage (repository root; see the README for environment variables)::

    m4x.py vcheck --out docs/ese3990/M4X_vcheck.json   # V on held-out RuleSailor
    m4x.py freeze                                      # before any X run
    m4x.py run --stage validation --out runs/ese3990/M4X/x2_validation --jobs 3
    m4x.py run --stage heldout --out runs/ese3990/M4X/heldout --jobs 3

X1/X1b are ``train.py --critic-layernorm`` runs (runs/ese3990/M4X/main/launch.sh).
X2 is the H=40 CEM-MPC of M3/M4U with a terminal value: V of PPO + BC's
initial checkpoint for the same seed, fitted in M4U and never refitted.
"""

from __future__ import annotations

import argparse
import json
import multiprocessing
import os
import platform
import time
from concurrent.futures import ProcessPoolExecutor
from datetime import UTC, datetime
from pathlib import Path

import demos
import jax
import m4u_compare
import numpy as np
import ppo
from compare import digest, write_freeze
from evaluate import run_episode, select_checkpoint
from mpc import CEMMPC, CEMConfig
from sailing import HERE, NOMINAL, REPO, Task, contract_identity, load_manifest
from viz.episode_store import EpisodeWriter, build_identity

FREEZE = HERE / "configs/M4X_freeze.json"
MAIN = REPO / "runs/ese3990/M4X/main"
M4U_TRAIN_FREEZE = HERE / "configs/M4U_train_freeze.json"
M4U_HELDOUT_FREEZE = REPO / "runs/ese3990/M4U/heldout/freeze.json"
DIAGNOSTICS = HERE / "configs/M4U_diagnostics.json"
VAL_STORE = REPO / "runs/ese3990/M4U/eval/store_rule_sailor_validation"
SEEDS = (0, 1, 2)
LABEL = "M4X: post-hoc, exploratory (U validation outcomes were open before M4X)"
STAGES = {"validation": ("validation",), "heldout": ("nominal", "test", "shift")}
SOURCES = [
    f"experiments/ese3990/{f}"
    for f in [
        *("m4x.py", "sac.py", "mpc.py", "train.py", "ppo.py", "demos.py"),
        *("sailing.py", "evaluate.py", "baselines.py", "m4u_compare.py"),
        *("compare.py", "viz/episode_store.py", "viz/replay.py"),
    ]
]
# The plan's "Readings declared before results", verbatim.
READINGS = [
    [
        "X1 bounded, and finishes > SAC + demonstrations",
        "The critic blowing up was a cause of the upwind failure",
    ],
    [
        "X1 bounded, finishes unchanged",
        "The blow-up was a symptom; the bottleneck is elsewhere "
        "(exploration, the timeout trap)",
    ],
    [
        "X1 still unbounded",
        "LayerNorm alone does not stabilize SAC at γ = 0.99975; "
        "report it, with no further fixes in M4X",
    ],
    [
        "X2 finishes ≈ H=160 in real time",
        "A learned value substitutes for foresight: "
        "learning buys the planner back real time",
    ],
    [
        "X2 tacks but finishes less often than H=160",
        "V points the right way but is imprecise near the gate",
    ],
    [
        "X2 never tacks, like H=40",
        "V does not encode the worth of a tack; the horizon gap remains",
    ],
]


def rel(path: Path) -> str:
    return os.path.relpath(path, REPO)


def file_hashes(ckpt: Path) -> dict[str, str]:
    return {s: digest(ckpt.with_suffix(s)) for s in (".json", ".npz")}


def value_checkpoints() -> dict[int, Path]:
    """PPO + BC's initial checkpoints: the frozen M4U ``bc_only`` groups."""
    groups = json.loads(M4U_HELDOUT_FREEZE.read_text())["groups"]
    found = {
        g["training_seed"]: REPO / g["checkpoint"]
        for g in groups
        if "bc_only" in g["roles"]
    }
    assert sorted(found) == list(SEEDS), found
    return found


_value = jax.jit(ppo.value)


class ValueTail:
    """X2's terminal value V(φ(s)) on research states ``(K, n)``, with the
    checkpoint's own feature normalization. Fitted in M4U by regression to
    the 50 demonstrations' discounted returns-to-go; no bootstrapping."""

    def __init__(self, checkpoint: Path, task: Task) -> None:
        self.params, meta = ppo.load(checkpoint)
        self.norm, self.clip = meta["normalization"], meta["config"]["norm_clip"]
        self.target, self.checkpoint = task.target, checkpoint
        self._identity = {
            "value": "critic of PPO + BC's initial checkpoint (M4U), not refitted",
            "checkpoint": rel(checkpoint),
            "sha256": file_hashes(checkpoint),
        }

    def __call__(self, rs: np.ndarray) -> np.ndarray:
        obs = ppo.normalize(rs, self.target, self.norm, self.clip)
        return np.asarray(_value(self.params, obs), dtype=np.float64)

    def identity(self) -> dict:
        return self._identity


# ---------------------------------------------------------------------------
# V check (plan M4X): returns-to-go of held-out RuleSailor episodes
# ---------------------------------------------------------------------------


def vcheck() -> dict:
    """V of each seed on RuleSailor's 20 U validation episodes (never used in
    fitting) against a constant predictor: the mean of V's own fitting
    targets. Reported either way; V is not refitted."""
    task = Task.load("U")
    d = json.loads(M4U_TRAIN_FREEZE.read_text())["configs"]["sac_demo"]["demos"]
    demo = demos.load(REPO / d["store"], d["sha256"], task)
    val_sha = demos.store_sha256(VAL_STORE)
    val = demos.load(VAL_STORE, val_sha, task, kind="evaluation")
    ret = val["return_to_go"]
    const = float(demo["return_to_go"].mean())
    starts = np.r_[0, np.flatnonzero(val["cont"][:-1] == 0) + 1]
    assert len(starts) == 20, len(starts)
    mse_const = float(np.mean((ret - const) ** 2))
    out = {
        "label": LABEL,
        "data": {
            "store": rel(VAL_STORE),
            "sha256": val_sha,
            "episodes": len(starts),
            "decisions": len(ret),
        },
        "targets": "discounted return-to-go under U contract 2 (γ = 0.99975), "
        "truncated sum at a timeout: the convention V was fitted with",
        "constant_predictor": const,
        "mse_constant": mse_const,
        "start_return": ret[starts].tolist(),
        "seeds": {},
    }
    for seed, ckpt in value_checkpoints().items():
        tail = ValueTail(ckpt, task)
        v = tail(val["rs"])
        mse = float(np.mean((v - ret) ** 2))
        out["seeds"][str(seed)] = {
            **tail.identity(),
            "mse": mse,
            "mse_ratio_to_constant": mse / mse_const,
            "correlation": float(np.corrcoef(v, ret)[0, 1]),
            "fit_mse_on_demonstrations": float(
                np.mean((tail(demo["rs"]) - demo["return_to_go"]) ** 2)
            ),
            "start_value": v[starts].tolist(),
        }
    return out


# ---------------------------------------------------------------------------
# Freeze: written before any X run
# ---------------------------------------------------------------------------


def make_freeze(vcheck_path: Path | None) -> dict:
    task = Task.load("U")
    m4u = json.loads(M4U_TRAIN_FREEZE.read_text())["configs"]
    mpc_cfg = json.loads((HERE / "configs/M3_freeze.json").read_text())["config"]
    assert (mpc_cfg["horizon"], mpc_cfg["candidates"], mpc_cfg["iterations"]) == (
        40,
        32,
        3,
    )
    gamma = 0.99975
    assert task.contract_version == 2
    return {
        "status": "FROZEN before any M4X run",
        "written_utc": datetime.now(UTC).isoformat(timespec="seconds"),
        "label": LABEL,
        "task": "U",
        **contract_identity(task),
        "identity": build_identity(),
        "arms": {
            "X1": {
                "arm": "sac_demo_ln",
                "parent": "sac_demo (M4U SAC + demonstrations)",
                "change": "LayerNorm (learned scale and offset, eps 1e-6) between "
                "the Dense and the tanh of each hidden layer of both critics and "
                "their Polyak targets, in RLPD's order (Ball et al. 2023). Actor, "
                "demonstrations, seeds, budget, replay, γ, checkpoints and "
                "selection unchanged",
                "seeds": list(SEEDS),
                "required": True,
            },
            "X1b": {
                "arm": "sac_ln",
                "parent": "sac (M4U SAC from scratch)",
                "change": "the same LayerNorm change",
                "seeds": list(SEEDS),
                "required": False,
            },
            "X2": {
                "arm": "x2",
                "parent": "M4U MPC H40_K32 (the frozen M3 default under U's γ)",
                "change": "score += γ^H·V(φ(s_H)) for candidates still running "
                "after H decisions; no tail after a finish, failure or the time "
                "limit. K, I, blocks, warm start and search seeds unchanged",
                "search_seeds": list(SEEDS),
                "value": "search seed s uses PPO + BC seed s's initial critic "
                "and normalization",
                "required": True,
            },
        },
        "configs": {
            "sac_demo_ln": {
                **m4u["sac_demo"],
                "critic_layernorm": True,
                "arm": "sac_demo_ln",
            },
            "sac_ln": {**m4u["sac"], "critic_layernorm": True, "arm": "sac_ln"},
        },
        "x2": {
            "config": mpc_cfg,
            "search_seeds": list(SEEDS),
            "value_checkpoints": {
                str(s): {"checkpoint": rel(p), "sha256": file_hashes(p)}
                for s, p in value_checkpoints().items()
            },
        },
        "thresholds": {
            "bounded_as_declared": "max |Q| ≤ 250 (10× R's largest) at every "
            "logged window, and mean Q never above +20 (plan M4X)",
            "bounded_entropy_adjusted": {
                "rule": "Q' = Q − o_t, o_t = α_t·H_t·γ/(1−γ) from window t's mean "
                "temperature α_t and policy entropy H_t: the discounted entropy "
                "term of SAC's soft value if α and H held. A critic is bounded if "
                "max(q_max − o_t, o_t − q_min) ≤ 250 and mean Q − o_t ≤ +20 at "
                "every logged window (2,048 updates)",
                "gamma": gamma,
                "why": "added 2026-11-17 before any X run (user decision): at "
                "γ = 0.99975 an accurate soft critic reads ≈ task return − 8000·α "
                "when H sits at its −2 target, so the declared rule cannot tell "
                "a blow-up from a large α",
                "used_for_readings": True,
            },
            "logging": "new runs add per-window q_max and q_min (minibatch "
            "extremes) to metrics.jsonl; instrumentation only, training unchanged",
            "real_time": "p50 decision latency < 50 ms on U validation, 50 ms "
            "deadline misses reported (as M3/M4)",
            "latency_conditions": "RAYON_NUM_THREADS=4, 3 worker processes (the "
            "M4U sweep's settings) and no other experiment process running for "
            "the validation run; the tail's own wall time is logged. M4U's H=40 "
            "and H=160 latencies were measured while 12 training runs shared "
            "the CPU (AMD Ryzen AI 9 HX 370, 12 cores of two kinds)",
            "predicted_vs_executed": "X2 records each decision's best planned "
            "score (tail included) and compares it with the discounted "
            "return-to-go actually received, from exact replay",
        },
        "diagnostics_thresholds": {rel(DIAGNOSTICS): digest(DIAGNOSTICS)},
        "vcheck": {rel(vcheck_path): digest(vcheck_path)} if vcheck_path else None,
        "selection": m4u_compare.SELECTION,
        "stages": {k: list(v) for k, v in STAGES.items()},
        "environment_seed": 0,
        "readings": READINGS,
        "cut_order": ["X1b", "X2 held-out (keep validation)", "X1 held-out"],
        "source_sha256": {f: digest(REPO / f) for f in SOURCES},
    }


# ---------------------------------------------------------------------------
# Evaluation: X2 on a stage; on held-out also every X1/X1b checkpoint
# ---------------------------------------------------------------------------


def learner_groups(fz: dict, main: Path = MAIN) -> tuple[list[dict], dict]:
    """M4U's checkpoint groups for every M4X learner arm whose three runs are
    complete (X1b is optional), checked against the freeze."""
    ident, contract = (
        fz["identity"],
        {k: fz.get(k) for k in contract_identity(Task.load("U"))},
    )
    groups, hashes = [], {}
    for arm, cfg in fz["configs"].items():
        runs = [main / f"{arm}_U_s{seed}" for seed in SEEDS]
        if not all((r / "summary.json").exists() for r in runs):
            print(f"{arm}: runs incomplete, not evaluated")
            continue
        for seed, run in zip(SEEDS, runs, strict=True):
            config = json.loads((run / "config.json").read_text())
            curve = m4u_compare.read_jsonl(run / "curve.jsonl")
            store = json.loads((run / "store" / "run.json").read_text())
            if config["config"] != cfg or {k: config.get(k) for k in contract} != (
                contract
            ):
                raise ValueError(f"{run}: settings differ from the M4X freeze")
            if store["identity"] != ident:
                raise ValueError(f"{run}: trained on a different build")
            if len(curve) != len(cfg["checkpoints"]):
                raise ValueError(f"{run}: missing checkpoints")
            selected = select_checkpoint(curve)["decisions"]
            for row in curve:
                ckpt = run / "ckpt" / f"{row['decisions']:07d}"
                hashes.update({rel(ckpt) + s: h for s, h in file_hashes(ckpt).items()})
                roles = ["curve"]
                roles += ["selected"] if row["decisions"] == selected else []
                roles += ["final"] if row["decisions"] == cfg["decisions"] else []
                groups.append(
                    {
                        "id": f"{arm}_s{seed}_d{row['decisions']:07d}",
                        "arm": arm,
                        "controller": "sac",
                        "training_seed": seed,
                        "checkpoint": rel(ckpt),
                        "decisions": row["decisions"],
                        "roles": roles,
                    }
                )
    return groups, hashes


def prepare(stage: str) -> dict:
    fz = json.loads(FREEZE.read_text())
    stale = [f for f, h in fz["source_sha256"].items() if digest(REPO / f) != h]
    if stale:
        raise ValueError(f"source changed since the M4X freeze: {stale}")
    if fz["identity"] != build_identity():
        raise ValueError("physics build differs from the M4X freeze")
    x2 = fz["x2"]
    for s in x2["value_checkpoints"].values():
        if file_hashes(REPO / s["checkpoint"]) != s["sha256"]:
            raise ValueError(f"{s['checkpoint']}: V checkpoint changed")
    groups = [
        {
            "id": f"x2_search{seed}",
            "arm": "x2",
            "controller": "mpc_tail",
            "search_seed": seed,
            "config": x2["config"],
            "value_checkpoint": x2["value_checkpoints"][str(seed)]["checkpoint"],
            "roles": ["selected"],
        }
        for seed in x2["search_seeds"]
    ]
    hashes = {}
    if stage == "heldout":
        more, hashes = learner_groups(fz)
        groups += more
    names = ("validation", "test", "shift")
    return {
        "label": LABEL,
        "stage": stage,
        "task": "U",
        **{k: fz[k] for k in ("contract_version", "contract_amendment_sha256")},
        "identity": fz["identity"],
        "m4x_freeze_sha256": digest(FREEZE),
        "groups": groups,
        "checkpoint_hashes": hashes,
        "diagnostics_thresholds": fz["diagnostics_thresholds"],
        "condition_hashes": {
            n: digest(HERE / "configs/conditions" / f"{n}.json") for n in names
        },
        "evaluation_manifests": list(STAGES[stage]),
        "environment_seed": 0,
        "selection": fz["selection"],
        "evaluator_sha256": digest(Path(__file__)),
    }


def run_group(out: Path, group: dict, task: Task, conditions: dict) -> int:
    """One group's missing episodes. Learner groups use M4U's evaluator
    unchanged; X2 additionally records each decision's predicted score."""
    if group["controller"] != "mpc_tail":
        return m4u_compare.run_group(out, group, task, conditions)
    path = out / "episodes" / f"{group['id']}.jsonl"
    done = {(e["manifest"], e["condition"]["id"]) for e in m4u_compare.read_jsonl(path)}
    todo = [(m, c) for m, cs in conditions.items() for c in cs if (m, c.id) not in done]
    if not todo:
        return 0
    tail = ValueTail(REPO / group["value_checkpoint"], task)
    ctrl = CEMMPC(group["search_seed"], CEMConfig(**group["config"]), tail)
    path.parent.mkdir(parents=True, exist_ok=True)
    meta = {"group": group, "freeze_sha256": digest(out / "freeze.json")}
    with EpisodeWriter(out / "store" / group["id"], meta, shard_size=1) as w:
        for manifest, cond in todo:
            log = {
                "group": group["id"],
                "manifest": manifest,
                "kind": "evaluation",
                "roles": group["roles"],
            }
            ep = run_episode(task, cond, ctrl, seed=0, writer=w, log_meta=log)
            ep.update(
                group=group["id"],
                manifest=manifest,
                predicted=ctrl.predicted,
                tail_s=ctrl.tail_s,
            )
            with path.open("a") as f:
                f.write(json.dumps(ep) + "\n")
    print(f"completed {group['id']} ({len(todo)} episodes)", flush=True)
    return len(todo)


def run(out: Path, frozen: dict, jobs: int = 1) -> None:
    """As ``m4u_compare.run``, with :func:`run_group`; resumable."""
    write_freeze(out, frozen)  # before any episode of this stage exists
    conditions = {
        m: [NOMINAL] if m == "nominal" else load_manifest(m)
        for m in frozen["evaluation_manifests"]
    }
    task = Task.load("U", frozen["contract_version"])
    with (out / "launches.jsonl").open("a") as f:
        launch = {
            "time": time.time(),
            "jobs": jobs,
            "rayon_threads": os.environ.get("RAYON_NUM_THREADS", "automatic"),
            "xla_flags": os.environ.get("XLA_FLAGS"),
            "hardware": platform.processor(),
            "load_average": os.getloadavg(),
        }
        f.write(json.dumps(launch) + "\n")
    groups = frozen["groups"]  # X2 first: the longest groups start first
    n = len(groups)
    args = ([out] * n, groups, [task] * n, [conditions] * n)
    if jobs == 1:
        list(map(run_group, *args))
    else:
        spawn = multiprocessing.get_context("spawn")  # no inherited JAX/Rayon state
        with ProcessPoolExecutor(jobs, mp_context=spawn) as ex:
            list(ex.map(run_group, *args))
    episodes = sum(
        len(m4u_compare.read_jsonl(out / "episodes" / f"{g['id']}.jsonl"))
        for g in groups
    )
    (out / "complete.json").write_text(
        json.dumps({"groups": n, "episodes": episodes}, indent=2) + "\n"
    )


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    sub = ap.add_subparsers(dest="cmd", required=True)
    v = sub.add_parser("vcheck")
    v.add_argument("--out", type=Path, required=True)
    f = sub.add_parser("freeze")
    f.add_argument("--vcheck", type=Path, help="the V check written before it")
    r = sub.add_parser("run")
    r.add_argument("--stage", choices=list(STAGES), required=True)
    r.add_argument("--out", type=Path, required=True)
    r.add_argument("--jobs", type=int, default=1)
    a = ap.parse_args(argv)
    if a.cmd == "vcheck":
        a.out.write_text(json.dumps(vcheck(), indent=1) + "\n")
        print(a.out.read_text()[:1500])
    elif a.cmd == "freeze":
        if FREEZE.exists():
            raise SystemExit(f"{FREEZE} exists; a freeze is never rewritten")
        FREEZE.write_text(json.dumps(make_freeze(a.vcheck), indent=1) + "\n")
        print(f"wrote {FREEZE}")
    else:
        run(a.out, prepare(a.stage), a.jobs)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
