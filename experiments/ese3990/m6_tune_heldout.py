"""Held-out evaluation of M6 tuning runs.

Each run's checkpoint is selected on the pilot panel only (most finishes; ties
go to the later checkpoint), then evaluated with ``m6_tune_eval.py`` on the
frozen validation, test and shift manifests. The final checkpoint is
evaluated too.

    .venv/bin/python experiments/ese3990/m6_tune_heldout.py OUT.json RUN [RUN ...]
        [--workers 8] [--budget DECISIONS]
"""

import argparse
import json
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from m6_tune import OUT

SPLITS = ("validation", "test", "shift")


def select(run: Path, budget: float):
    rows = [json.loads(x) for x in (run / "log.jsonl").read_text().splitlines()]
    rows = [r for r in rows if r["decisions"] <= budget]
    best = max(rows, key=lambda r: (r["eval"].get("finished", 0), r["decisions"]))
    return best["decisions"], best["eval"].get("finished", 0), rows[-1]["decisions"]


def job(run, ckpt, split):
    out = run / f"eval_ckpt_{ckpt:09d}_{split}" / "summary.json"
    if not out.exists():
        subprocess.run(
            [
                sys.executable,
                "m6_tune_eval.py",
                str(run),
                str(ckpt),
                split,
                "--jobs",
                "1",
            ],
            check=True,
            capture_output=True,
            cwd=Path(__file__).parent,
        )
    s = json.loads(out.read_text())
    o, d = s["outcomes"], s["diagnostics"]
    return dict(
        episodes=o["episodes"],
        finished=o["finished"],
        outcomes=o["outcome_counts"],
        finish_time_mean=(o["finish_time_s_given_success"] or {}).get("mean"),
        episodes_with_a_tack=d["episodes_with_a_tack"],
        finished_with_tack=s["finished_with_tack"],
        replayed_exactly=s["replayed_exactly"],
    )


def main():
    p = argparse.ArgumentParser()
    p.add_argument("out", type=Path)
    p.add_argument("runs", nargs="+")
    p.add_argument("--workers", type=int, default=8)
    p.add_argument(
        "--budget",
        type=float,
        default=float("inf"),
        help="only checkpoints at or below this many decisions",
    )
    a = p.parse_args()
    runs = [OUT / r for r in a.runs]
    plan = []
    meta = {}
    for run in runs:
        sel, panel, final = select(run, a.budget)
        meta[run.name] = dict(
            selected_ckpt=sel, selected_panel_finishes=panel, final_ckpt=final
        )
        for which, ck in (("selected", sel), ("final", final)):
            for split in SPLITS:
                plan.append((run, which, ck, split))
    unique = sorted({(run, ck, split) for run, _, ck, split in plan})
    with ThreadPoolExecutor(
        a.workers
    ) as ex:  # selected may equal final: run each store once
        done = dict(zip(unique, ex.map(lambda t: job(*t), unique), strict=True))
    for run, which, ck, split in plan:
        meta[run.name].setdefault(which, {})[split] = done[(run, ck, split)]
    a.out.write_text(json.dumps(meta, indent=1))
    for name, m in meta.items():
        line = " ".join(
            f"{s[:4]} {m['selected'][s]['finished']}/{m['selected'][s]['episodes']}"
            for s in SPLITS
        )
        fin = " ".join(
            f"{m['final'][s]['finished']}/{m['final'][s]['episodes']}" for s in SPLITS
        )
        print(
            f"{name:26} sel {m['selected_ckpt'] / 1e6:.1f}M"
            f" ({m['selected_panel_finishes']}/40) {line} | final {fin}"
        )


if __name__ == "__main__":
    main()
