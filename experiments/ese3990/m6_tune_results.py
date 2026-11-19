"""Collect M6 tuning pilots and held-out checks into one JSON.

.venv/bin/python experiments/ese3990/m6_tune_results.py \
    docs/ese3990/M6_tune_results.json
"""

import json
import sys

from m6_tune import OUT

ARGS = (
    "learner",
    "norm",
    "head",
    "repeat",
    "hidden",
    "log_std",
    "lam",
    "ent",
    "lr",
    "n_envs",
    "rollout",
    "p_random",
    "equivariant",
    "noise_beta",
    "train_fail",
    "anneal",
    "target_kl",
    "target_entropy",
    "utd",
    "nstep",
    "layernorm",
    "decisions",
)


def main(out):
    runs = {}
    for d in sorted(OUT.glob("*_s*")):
        if not (d / "log.jsonl").exists() or d.name.startswith("smoke"):
            continue
        rows = [json.loads(x) for x in (d / "log.jsonl").read_text().splitlines()]
        a = json.loads((d / "args.json").read_text())
        runs[d.name] = dict(
            config={k: a.get(k) for k in ARGS},
            seed=a["seed"],
            decisions=[r["decisions"] for r in rows],
            panel_finishes=[r["eval"].get("finished", 0) for r in rows],
            panel_max_y=[round(r["eval"]["maxy"], 1) for r in rows],
            train_finishes=[(r.get("train") or {}).get("finished", 0) for r in rows],
            train_episodes=[(r.get("train") or {}).get("n", 0) for r in rows],
            wall_s=rows[-1]["wall"],
        )
    heldout = {
        p.stem: json.loads(p.read_text())
        for p in sorted((OUT / "heldout").glob("*.json"))
    }
    doc = {
        "label": "M6 tuning pilots (post-hoc, exploratory); pilot panel = 40 "
        "training-distribution conditions (TrainingSampler(90000)) from the "
        "original start, deterministic policy",
        "runs": runs,
        "heldout": heldout,
    }
    with open(out, "w") as f:
        f.write(json.dumps(doc, indent=1))
    print(f"{len(runs)} runs, {len(heldout)} held-out groups -> {out}")


if __name__ == "__main__":
    main(sys.argv[1])
