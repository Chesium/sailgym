"""Finish curves of M6 tuning pilots: finishes /40 per eval, last max y and
training finishes.

.venv/bin/python experiments/ese3990/m6_tune_report.py [PREFIX ...]
"""

import json
import sys

from m6_tune import OUT

for prefix in sys.argv[1:] or [""]:
    for d in sorted(OUT.glob(f"{prefix}*_s*")):
        if not (d / "log.jsonl").exists():
            continue
        rows = [json.loads(x) for x in (d / "log.jsonl").read_text().splitlines()]
        t = rows[-1].get("train") or {}
        curve = " ".join(f"{r['eval'].get('finished', 0):2d}" for r in rows)
        print(
            f"{d.name:24} {curve} | {rows[-1]['decisions'] / 1e6:4.1f}M"
            f" maxy {rows[-1]['eval']['maxy']:.0f}"
            f" train {t.get('finished', 0)}/{t.get('n', 0)}"
        )
