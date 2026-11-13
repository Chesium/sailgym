"""Figures from raw run summaries (plan §5, §7).

Usage::

    uv run --frozen --no-sync python experiments/ese3990/plots.py \
        runs/ese3990/ppo/R_s0_1M --out docs/ese3990/figs/ppo_R_curve.png
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402


def learning_curve(runs: list[Path], out: Path) -> None:
    """Validation success rate and mean return against real decisions, one
    line per run (individual seeds, never only a mean: plan E3)."""
    fig, (ax_s, ax_r) = plt.subplots(1, 2, figsize=(10, 3.6), layout="constrained")
    for run in runs:
        rows = [json.loads(x) for x in (run / "curve.jsonl").read_text().splitlines()]
        d = [r["decisions"] for r in rows]
        ax_s.plot(d, [r["val_success_rate"] for r in rows], marker="o", label=run.name)
        ax_r.plot(d, [r["val_mean_return"] for r in rows], marker="o", label=run.name)
    ax_s.set(xlabel="real decisions (incl. calibration)", ylabel="validation success")
    ax_s.set_ylim(-0.05, 1.05)
    ax_r.set(
        xlabel="real decisions (incl. calibration)", ylabel="validation mean return"
    )
    ax_s.legend(fontsize="small")
    out.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(out, dpi=150)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("runs", nargs="+", type=Path)
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args(argv)
    learning_curve(args.runs, args.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
