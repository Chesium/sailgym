"""Generate the frozen condition manifests (plan §E3).

    uv run python experiments/ese3990/make_conditions.py [--check]

Writes ``configs/conditions/{training,validation,test,shift}.json``. The
generator is deterministic (numpy PCG64 from a declared integer per split), and
values are rounded so that a condition is an exact document: ``--check``
regenerates in memory and fails if a committed file differs, and verifies that
no two documents across validation/test/shift share content.

Training conditions are not enumerated: ``sample_training`` draws them from the
declared ranges for a given (training seed, episode index), rejecting any that
coincide with a validation/test/shift document.
"""

from __future__ import annotations

import argparse
import json
import sys
from typing import Any

import numpy as np
from sailing import CONDITIONS_DIR, Condition

GENERATOR_VERSION = 1
SPEED_DP = 3  # m/s rounded to 1e-3
ANGLE_DP = 2  # degrees rounded to 1e-2

TRAIN_RANGE = {
    "wind_speed": (4.5, 5.5),
    "wind_bearing_deg": (-5.0, 5.0),
    "heading_deg": (75.0, 105.0),
}
# Shift: both speed and initial heading outside the training range.
SHIFT = {
    "wind_speed_values": (4.0, 6.0),
    "wind_bearing_deg": (-5.0, 5.0),
    "heading_offset_abs_deg": (15.0, 30.0),
}
SPLIT_SEEDS = {"validation": 1001, "test": 2002, "shift": 3003, "training": 4004}
SPLIT_SIZES = {"validation": 20, "test": 50, "shift": 20}


def _round(speed: float, bearing: float, heading: float) -> tuple[float, float, float]:
    return (round(speed, SPEED_DP), round(bearing, ANGLE_DP), round(heading, ANGLE_DP))


def _in_range(rng: np.random.Generator) -> tuple[float, float, float]:
    r = TRAIN_RANGE
    return _round(
        rng.uniform(*r["wind_speed"]),
        rng.uniform(*r["wind_bearing_deg"]),
        rng.uniform(*r["heading_deg"]),
    )


def _split(name: str, n: int, exclude: set) -> list[Condition]:
    rng = np.random.default_rng(SPLIT_SEEDS[name])
    out: list[Condition] = []
    seen = set(exclude)
    while len(out) < n:
        if name == "shift":
            s = SHIFT
            speed = s["wind_speed_values"][len(out) % 2]
            sign = 1.0 if rng.uniform() < 0.5 else -1.0
            k = _round(
                speed,
                rng.uniform(*s["wind_bearing_deg"]),
                90.0 + sign * rng.uniform(*s["heading_offset_abs_deg"]),
            )
        else:
            k = _in_range(rng)
        if k in seen:
            continue
        seen.add(k)
        out.append(Condition(f"{name}-{len(out):03d}", *k))
    return out


def generate() -> dict[str, dict[str, Any]]:
    docs: dict[str, dict[str, Any]] = {}
    used: set = set()
    for name in ("validation", "test", "shift"):
        conds = _split(name, SPLIT_SIZES[name], used)
        used |= {c.key() for c in conds}
        docs[name] = {
            "generator_version": GENERATOR_VERSION,
            "split": name,
            "seed": SPLIT_SEEDS[name],
            "rounding": {"wind_speed_dp": SPEED_DP, "angle_dp": ANGLE_DP},
            "ranges": SHIFT if name == "shift" else TRAIN_RANGE,
            "conditions": [c.to_dict() for c in conds],
        }
    docs["training"] = {
        "generator_version": GENERATOR_VERSION,
        "split": "training",
        "seed": SPLIT_SEEDS["training"],
        "rounding": {"wind_speed_dp": SPEED_DP, "angle_dp": ANGLE_DP},
        "ranges": TRAIN_RANGE,
        "sampling": (
            "numpy default_rng([seed, training_seed]); one uniform triple "
            "(speed, bearing, heading) per episode in order; rounded; triples "
            "equal to any validation/test/shift document are rejected and redrawn"
        ),
        "excluded_documents": len(used),
        "conditions": [],
    }
    return docs


def held_out_keys() -> set:
    keys = set()
    for name in ("validation", "test", "shift"):
        data = json.loads((CONDITIONS_DIR / f"{name}.json").read_text())
        keys |= {Condition.from_dict(c).key() for c in data["conditions"]}
    return keys


class TrainingSampler:
    """Training conditions for one training seed, in episode order."""

    def __init__(self, training_seed: int) -> None:
        self.training_seed = training_seed
        self.rng = np.random.default_rng([SPLIT_SEEDS["training"], training_seed])
        self.exclude = held_out_keys()
        self.count = 0

    def next(self) -> Condition:
        while True:
            k = _in_range(self.rng)
            if k not in self.exclude:
                c = Condition(f"train-s{self.training_seed}-{self.count:06d}", *k)
                self.count += 1
                return c


def check() -> list[str]:
    problems = []
    fresh = generate()
    for name, doc in fresh.items():
        path = CONDITIONS_DIR / f"{name}.json"
        if not path.exists():
            problems.append(f"{path} missing")
            continue
        if json.loads(path.read_text()) != json.loads(json.dumps(doc)):
            problems.append(f"{path} differs from the generator")
    keys = [
        Condition.from_dict(c).key()
        for name in ("validation", "test", "shift")
        for c in fresh[name]["conditions"]
    ]
    if len(keys) != len(set(keys)):
        problems.append("duplicate documents across held-out splits")
    return problems


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args(argv)
    if args.check:
        problems = check()
        for p in problems:
            print(p, file=sys.stderr)
        return 1 if problems else 0
    CONDITIONS_DIR.mkdir(parents=True, exist_ok=True)
    for name, doc in generate().items():
        (CONDITIONS_DIR / f"{name}.json").write_text(json.dumps(doc, indent=1) + "\n")
        print(f"wrote {name}: {len(doc['conditions'])} conditions")
    return 0


if __name__ == "__main__":
    sys.exit(main())
