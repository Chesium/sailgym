"""M4X report (plan §6): critic scale, X1/X1b validation curves, X2 outcomes,
latency and predicted-versus-executed return, and held-out rows when they
exist. Every row is labeled exploratory and kept apart from M4/M4U tables.

    python experiments/ese3990/m4x_report.py --out docs/ese3990

Each part runs only when its inputs exist, so the critic-scale figure can be
drawn from the 9 M4/M4U SAC runs before any X run finishes.
"""

from __future__ import annotations

import argparse
import json
import multiprocessing
import os
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import demos  # noqa: E402
import m4x  # noqa: E402
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402
import u_diagnostics  # noqa: E402
from compare import digest  # noqa: E402
from comparison_report import aggregate  # noqa: E402
from evaluate import select_checkpoint  # noqa: E402
from m4u_compare import read_jsonl  # noqa: E402
from m4u_report import diagnostics_table, outcome_table  # noqa: E402
from mpc_study import cost_summary  # noqa: E402
from sailing import REPO, RS, Task, load_manifest  # noqa: E402
from viz.episode_store import EpisodeStore  # noqa: E402

RUNS = REPO / "runs/ese3990"
X2_VAL = RUNS / "M4X/x2_validation"
HELDOUT = RUNS / "M4X/heldout"
ARMS = {  # arm -> (label, run directory pattern, colour, line style)
    "sac_R": ("SAC on R (M4)", "main/sac_R_s{}", "0.55", "-"),
    "sac": ("SAC on U (M4U)", "M4U/main/sac_U_s{}", "tab:blue", "-"),
    "sac_demo": (
        "SAC + demos on U (M4U, expert data)",
        "M4U/main/sac_demo_U_s{}",
        "tab:orange",
        "-",
    ),
    "sac_ln": (
        "X1b: SAC + critic LN (M4X, exploratory)",
        "M4X/main/sac_ln_U_s{}",
        "tab:cyan",
        "--",
    ),
    "sac_demo_ln": (
        "X1: SAC + demos + critic LN (M4X, exploratory, expert data)",
        "M4X/main/sac_demo_ln_U_s{}",
        "tab:red",
        "--",
    ),
}
LABELS = {
    "x2": "X2: MPC H=40 + value tail (M4X, exploratory)",
    **{k: v[0] for k, v in ARMS.items()},
}
LIMIT_ABS, LIMIT_MEAN = 250.0, 20.0
BURN_IN = 400_000  # M4X freeze amendment 1


def runs_of(arm: str) -> list[Path]:
    pattern = ARMS[arm][1]
    return [
        RUNS / pattern.format(s)
        for s in m4x.SEEDS
        if (RUNS / pattern.format(s)).is_dir()
    ]


def task_band(task: Task) -> tuple[float, float]:
    """Bounds on the discounted task return from any in-bounds state: the
    terminal ±10, the telescoped shaping (at most the largest gate distance
    over 30 m) and, below, the time cost of a run that never ends."""
    r = task.reward
    x0, y0, x1, y1 = task.bounds
    tx, ty = task.target
    d = max(np.hypot(x - tx, y - ty) for x in (x0, x1) for y in (y0, y1))
    shaping = d / r["distance_scale_m"]
    time = task.decision_dt / r["time_scale_s"] / (1 - r["gamma"])
    return -(r["failure_penalty"] + shaping + time), r["finish_bonus"] + shaping


# ---------------------------------------------------------------------------
# Critic scale (all SAC runs on R and U, existing logs plus M4X)
# ---------------------------------------------------------------------------


def critic_scale(run: Path, gamma: float) -> dict:
    """Per logged window: raw scale, entropy offset o = α·H·γ/(1−γ), and the
    declared and entropy-adjusted bounded tests (configs/M4X_freeze.json and
    its amendment 1: excess over the offsets seen since 400k decisions)."""
    m = read_jsonl(run / "metrics.jsonl")
    get = lambda k: np.array([r[k] for r in m], dtype=float)  # noqa: E731
    d = get("decisions")
    o = get("alpha") * get("entropy") * gamma / (1 - gamma)
    qm, qa = get("q_mean"), get("q_abs_max")
    test = d >= BURN_IN
    lo = np.minimum.accumulate(np.where(test, np.minimum(o, 0), 0))
    hi = np.maximum.accumulate(np.where(test, np.maximum(o, 0), 0))
    mean_excess = np.where(test, qm - hi, np.nan)
    exact = "q_max" in m[0]
    if exact:
        spread = spread_hi = np.maximum(get("q_max") - hi, lo - get("q_min"))
    else:  # before M4X only max|Q| was logged: bound the excess from both sides
        spread, spread_hi = qa - np.maximum(hi, -lo), qa - np.minimum(hi, -lo)
    spread, spread_hi = (np.where(test, x, np.nan) for x in (spread, spread_hi))
    if not test.any():
        verdict = None  # not yet past the burn-in
    elif np.nanmax(mean_excess) > LIMIT_MEAN or np.nanmax(spread) > LIMIT_ABS:
        verdict = False
    else:  # None: extremes not logged and the bounds straddle 250
        verdict = True if np.nanmax(spread_hi) <= LIMIT_ABS else None
    nanmax = lambda x: float(np.nanmax(x)) if test.any() else None  # noqa: E731
    return {
        "run": os.path.relpath(run, REPO),
        "decisions": d.tolist(),
        "q_abs_max": qa.tolist(),
        "q_mean": qm.tolist(),
        "offset": o.tolist(),
        "mean_excess": mean_excess.tolist(),
        "spread_excess": spread.tolist(),
        "spread_exact": exact,
        "summary": {
            "logged_decisions": int(d[-1]),
            "max_abs_q": float(qa.max()),
            "max_mean_q": float(qm.max()),
            "final_mean_q": float(qm[-1]),
            "final_alpha": float(get("alpha")[-1]),
            "final_entropy": float(get("entropy")[-1]),
            "final_offset": float(o[-1]),
            "admissible_offsets_since_400k": [float(lo[-1]), float(hi[-1])],
            "max_mean_excess": nanmax(mean_excess),
            "max_spread_excess": [nanmax(spread), nanmax(spread_hi)],
            "spread_exact": exact,
            "bounded_as_declared": bool(
                qa.max() <= LIMIT_ABS and qm.max() <= LIMIT_MEAN
            ),
            "bounded_entropy_adjusted": verdict,
        },
    }


def critic_figure(scales: dict[str, list[dict]], bands: dict, path: Path) -> None:
    fig, axes = plt.subplots(2, 2, figsize=(12, 8), sharex=True)
    panels = [
        ("max |Q| per window", "q_abs_max", LIMIT_ABS, "log"),
        (
            "mean Q (solid) and entropy offset o (dotted)",
            "q_mean",
            LIMIT_MEAN,
            "symlog",
        ),
        ("mean Q − hi, from 400k (amendment 1)", "mean_excess", LIMIT_MEAN, "symlog"),
        (
            "|Q| beyond [lo, hi] (bound before M4X)",
            "spread_excess",
            LIMIT_ABS,
            "symlog",
        ),
    ]
    for ax, (title, key, limit, scale) in zip(axes.flat, panels, strict=True):
        for arm, rows in scales.items():
            _, _, colour, style = ARMS[arm]
            for i, r in enumerate(rows):
                x = np.array(r["decisions"]) / 1e6
                ax.plot(
                    x,
                    r[key],
                    color=colour,
                    ls=style,
                    lw=1,
                    label=LABELS[arm] if i == 0 else None,
                )
                if key == "q_mean":
                    ax.plot(x, r["offset"], color=colour, ls=":", lw=0.8)
        ax.axhline(limit, color="k", lw=0.8, ls=":")
        if key == "q_mean":
            lo, hi = bands["U"]
            ax.axhspan(lo, hi, color="tab:green", alpha=0.15, lw=0)
        ax.set_yscale(scale, **({"linthresh": 20} if scale == "symlog" else {}))
        ax.set_title(title, fontsize=9)
        ax.grid(alpha=0.2)
    for ax in axes[1]:
        ax.set_xlabel("decisions (millions, calibration and demonstrations included)")
    axes[0, 0].legend(fontsize=7, loc="lower right")
    lo, hi = bands["U"]
    fig.suptitle(
        "SAC critic scale. Black dotted: M4X thresholds (250; +20). Green: "
        f"feasible discounted U task return [{lo:.1f}, {hi:+.1f}]. o = α·H·γ/(1−γ),"
        " the entropy term of SAC's soft value;\n[lo, hi] = [min(0, o), max(0, o)]"
        " over windows since 400k decisions (freeze amendment 1). γ = 0.999 on R,"
        " 0.99975 on U. M4X rows are exploratory.",
        fontsize=9,
    )
    fig.tight_layout()
    fig.savefig(path, dpi=150)
    plt.close(fig)


# ---------------------------------------------------------------------------
# X1/X1b: training-time validation (train.py's curve.jsonl)
# ---------------------------------------------------------------------------


def curves() -> dict:
    out = {}
    for arm in ("sac", "sac_demo", "sac_ln", "sac_demo_ln"):
        rows = {}
        for run in runs_of(arm):
            curve = read_jsonl(run / "curve.jsonl")
            if not curve:
                continue
            rows[run.name] = {
                "decisions": [r["decisions"] for r in curve],
                "val_finished": [r["val_finished"] for r in curve],
                "val_outcome_counts": [r["val_outcome_counts"] for r in curve],
                "complete": (run / "summary.json").exists(),
                "selected": select_checkpoint(curve)["decisions"]
                if (run / "summary.json").exists()
                else None,
            }
        out[arm] = rows
    return out


def curve_figure(cv: dict, path: Path) -> None:
    fig, ax = plt.subplots(figsize=(7, 4))
    for arm, rows in cv.items():
        _, _, colour, style = ARMS[arm]
        for i, r in enumerate(rows.values()):
            ax.plot(
                np.array(r["decisions"]) / 1e6,
                r["val_finished"],
                color=colour,
                ls=style,
                marker="o",
                ms=3,
                label=LABELS[arm] if i == 0 else None,
            )
    ax.set_xlabel("decisions (millions)")
    ax.set_ylabel("U validation finishes (of 20)")
    ax.set_title(
        "Training-time U validation, every seed (M4X rows exploratory)", fontsize=9
    )
    ax.grid(alpha=0.2)
    ax.legend(fontsize=7)
    fig.tight_layout()
    fig.savefig(path, dpi=150)
    plt.close(fig)


# ---------------------------------------------------------------------------
# Evaluation stages (X2 validation; held-out)
# ---------------------------------------------------------------------------


def load_stage(run: Path) -> tuple[dict, list[dict]]:
    frozen = json.loads((run / "freeze.json").read_text())
    eps = [
        e
        for g in frozen["groups"]
        for e in read_jsonl(run / "episodes" / f"{g['id']}.jsonl")
    ]
    return frozen, eps


def verify(run: Path, frozen: dict, episodes: list[dict]) -> dict:
    """Frozen hashes unchanged, exactly the frozen episode set, every store on
    the frozen build, and learner nominal reloads equal to training."""
    hashes = {
        **frozen["checkpoint_hashes"],
        **frozen["diagnostics_thresholds"],
        os.path.relpath(m4x.FREEZE, REPO): frozen["m4x_freeze_sha256"],
        "experiments/ese3990/m4x.py": frozen["evaluator_sha256"],
        **{
            f"experiments/ese3990/configs/conditions/{n}.json": h
            for n, h in frozen["condition_hashes"].items()
        },
    }
    for name, h in hashes.items():
        if digest(REPO / name) != h:
            raise ValueError(f"frozen input changed: {name}")
    expected = {
        (g["id"], m, c)
        for g in frozen["groups"]
        for m in frozen["evaluation_manifests"]
        for c in (["nominal"] if m == "nominal" else [x.id for x in load_manifest(m)])
    }
    actual = [(e["group"], e["manifest"], e["condition"]["id"]) for e in episodes]
    if len(actual) != len(set(actual)) or set(actual) != expected:
        raise ValueError("missing, duplicate or unexpected evaluation episodes")
    reloads = 0
    for g in frozen["groups"]:
        if EpisodeStore(run / "store" / g["id"]).identity != frozen["identity"]:
            raise ValueError(f"{g['id']}: store built on a different build")
        if "checkpoint" in g:
            curve = read_jsonl((REPO / g["checkpoint"]).parent.parent / "curve.jsonl")
            rec = next(r for r in curve if r["decisions"] == g["decisions"])
            nom = next(
                e
                for e in episodes
                if e["group"] == g["id"] and e["manifest"] == "nominal"
            )
            if nom["return"] != rec["nominal_return"]:
                raise ValueError(f"{g['id']}: nominal reload differs from training")
            reloads += 1
    return {
        "episodes": len(expected),
        "frozen_hashes_unchanged": True,
        "nominal_checkpoint_reloads_verified": reloads,
    }


def diagnose(run: Path, frozen: dict, jobs: int) -> dict:
    """u_diagnostics on every stored episode (exact replay, frozen thresholds)."""
    (path,) = frozen["diagnostics_thresholds"]
    cfg = u_diagnostics.load_config(REPO / path)
    work = []
    for g in frozen["groups"]:
        store = run / "store" / g["id"]
        k = len(EpisodeStore(store).index)
        work += [(str(store), lo, min(lo + 8, k), cfg) for lo in range(0, k, 8)]
    spawn = multiprocessing.get_context("spawn")
    with ProcessPoolExecutor(jobs, mp_context=spawn) as ex:
        parts = list(ex.map(u_diagnostics._chunk, work))
    return {
        f"{Path(w[0]).name}/{eid}": d
        for w, part in zip(work, parts, strict=True)
        for eid, d in part.items()
    }


def prediction(run: Path, frozen: dict, episodes: list[dict]) -> dict:
    """X2's best planned score at each decision against the discounted return
    actually received from that decision (exact replay, V's own convention:
    a timeout's return is the truncated sum)."""
    task = Task.load("U", frozen["contract_version"])
    vcheck = json.loads((REPO / "docs/ese3990/M4X_vcheck.json").read_text())
    best_rule = max(vcheck["start_return"])  # RuleSailor's best validation start
    pred, real, start, outcome, tracks = [], [], [], [], {}
    for g in (g for g in frozen["groups"] if g["arm"] == "x2"):
        store = run / "store" / g["id"]
        data = demos.load(store, demos.store_sha256(store), task, kind="evaluation")
        v = m4x.ValueTail(REPO / g["value_checkpoint"], task)(data["rs"])
        i, seg = 0, {}
        for e in EpisodeStore(store).index:
            seg[e["episode_id"]] = slice(i, i + e["decisions"])
            i += e["decisions"]
        vmax, vend, yend = np.array(
            [
                (v[s].max(), v[s][-1], data["next_rs"][s][-1, RS["y"]])
                for s in seg.values()
            ]
        ).T
        tracks[g["id"]] = {
            "mean_max_v": float(vmax.mean()),
            "mean_final_v": float(vend.mean()),
            "mean_final_y_m": float(yend.mean()),
            "episodes": len(vmax),
            "episodes_max_v_above_best_rulesailor": int((vmax > best_rule).sum()),
        }
        for e in (e for e in episodes if e["group"] == g["id"]):
            p, r = np.array(e["predicted"]), data["return_to_go"][seg[e["episode_id"]]]
            assert len(p) == len(r) == e["decisions"]
            pred.append(p)
            real.append(r)
            start.append((p[0], r[0]))
            outcome.append(np.full(len(p), e["outcome"] == "finished"))
    p, r, fin = map(np.concatenate, (pred, real, outcome))
    gap = p - r
    s = np.array(start)

    def stats(mask):
        return {
            "decisions": int(mask.sum()),
            "mean_gap": float(gap[mask].mean()) if mask.any() else None,
            "median_gap": float(np.median(gap[mask])) if mask.any() else None,
            "rmse": float(np.sqrt(np.mean(gap[mask] ** 2))) if mask.any() else None,
        }

    return {
        "definition": "gap = predicted − executed; positive means the planner "
        "expected more than it received (optimism a planner could exploit)",
        "all": {
            **stats(np.ones_like(fin)),
            "correlation": float(np.corrcoef(p, r)[0, 1]),
        },
        "finished_episodes": stats(fin),
        "unfinished_episodes": stats(~fin),
        "episode_start": {
            "mean_predicted": float(s[:, 0].mean()),
            "mean_executed": float(s[:, 1].mean()),
        },
        "value_along_tracks": {
            "definition": "V (the tail's own value, per search seed) on every "
            "executed state; compared with RuleSailor's best validation start "
            "return (M4X_vcheck.json). The gate is at y = 40 m, the start at y = 0",
            "best_rulesailor_start_return": best_rule,
            "per_group": tracks,
        },
        "_arrays": (p, r, fin),
    }


def prediction_figure(pr: dict, path: Path) -> None:
    p, r, fin = pr["_arrays"]
    idx = np.random.default_rng(0).permutation(len(p))[:20000]
    fig, ax = plt.subplots(figsize=(5, 5))
    for mask, colour, name in (
        (fin, "tab:green", "finished episodes"),
        (~fin, "tab:red", "unfinished episodes"),
    ):
        k = idx[mask[idx]]
        ax.scatter(r[k], p[k], s=2, alpha=0.3, color=colour, label=name)
    lim = [min(p.min(), r.min()), max(p.max(), r.max())]
    ax.plot(lim, lim, "k:", lw=0.8)
    ax.set_xlabel("executed discounted return-to-go")
    ax.set_ylabel("predicted (best plan, γ^H·V tail included)")
    ax.set_title("X2 predicted vs executed return (exploratory)", fontsize=9)
    ax.legend(fontsize=7, markerscale=4)
    fig.tight_layout()
    fig.savefig(path, dpi=150)
    plt.close(fig)


def stage_rows(frozen: dict, episodes: list[dict], diags: dict) -> tuple[list, list]:
    """Outcome and diagnostics rows: X2 (all search seeds), then each learner
    arm's validation-selected and final checkpoints."""
    keys = [("x2", "selected")]
    keys += [(a, r) for a in ("sac_demo_ln", "sac_ln") for r in ("selected", "final")]
    out, diag = [], []
    for arm, role in keys:
        groups = [g for g in frozen["groups"] if g["arm"] == arm and role in g["roles"]]
        ids = {g["id"] for g in groups}
        for manifest in frozen["evaluation_manifests"] if groups else ():
            rows = [
                e for e in episodes if e["group"] in ids and e["manifest"] == manifest
            ]
            head = {
                "arm": arm,
                "role": role,
                "label": LABELS[arm],
                "manifest": manifest,
            }
            per_seed = {
                g["id"]: aggregate([e for e in rows if e["group"] == g["id"]])
                for g in groups
            }
            row = {**head, **aggregate(rows), "per_seed": per_seed}
            if arm == "x2":
                tail = sum(e["tail_s"] for e in rows)
                row["cost"] = {
                    **cost_summary(rows),
                    "tail_s": tail,
                    "tail_share_of_latency": tail
                    / sum(sum(e["decision_latency_s"]) for e in rows),
                }
            out.append(row)
            d = [diags[f"{e['group']}/{e['episode_id']}"] for e in rows]
            diag.append({**head, **u_diagnostics.summarize(d)})
    return out, diag


def evaluate_stage(run: Path, jobs: int) -> dict | None:
    if not (run / "complete.json").exists():
        return None
    frozen, episodes = load_stage(run)
    checks = verify(run, frozen, episodes)
    diags = diagnose(run, frozen, jobs)
    rows, diag = stage_rows(frozen, episodes, diags)
    return {
        "stage": frozen["stage"],
        "label": frozen["label"],
        "checks": checks,
        "launches": read_jsonl(run / "launches.jsonl"),
        "outcomes": rows,
        "diagnostics": diag,
        "prediction": prediction(run, frozen, episodes),
    }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--jobs", type=int, default=os.cpu_count())
    a = ap.parse_args()
    figs = a.out / "figs"
    figs.mkdir(parents=True, exist_ok=True)
    bands = {t: task_band(Task.load(t)) for t in ("R", "U")}
    gamma = {t: Task.load(t).reward["gamma"] for t in ("R", "U")}
    scales = {
        arm: [
            critic_scale(r, gamma["R" if arm == "sac_R" else "U"]) for r in runs_of(arm)
        ]
        for arm in ARMS
    }
    critic_figure(scales, bands, figs / "M4X_critic_scale.png")
    result = {
        "label": m4x.LABEL,
        "freeze": {os.path.relpath(m4x.FREEZE, REPO): digest(m4x.FREEZE)},
        "task_return_bands": bands,
        "critic_scale": {
            arm: [r["summary"] | {"run": r["run"]} for r in rows]
            for arm, rows in scales.items()
        },
    }
    cv = curves()
    result["validation_curves"] = cv
    curve_figure(cv, figs / "M4X_validation_curves.png")
    md = []
    for name, run in (("x2_validation", X2_VAL), ("heldout", HELDOUT)):
        stage = evaluate_stage(run, a.jobs)
        if stage is None:
            continue
        prediction_figure(stage["prediction"], figs / f"M4X_{name}_prediction.png")
        stage["prediction"].pop("_arrays")
        result[name] = stage
        md += [
            f"## {name} (M4X, exploratory)\n",
            outcome_table(stage["outcomes"]),
            "",
            diagnostics_table(stage["diagnostics"]),
            "",
        ]
    (a.out / "M4X_results.json").write_text(json.dumps(result, indent=1) + "\n")
    (a.out / "M4X_outcomes.md").write_text("\n".join(md))
    print(json.dumps(result["critic_scale"], indent=1)[:3000])


if __name__ == "__main__":
    main()
