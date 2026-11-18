"""Launch the five-seed SAC decision-budget study using the frozen M5 trainer."""

import argparse
import fcntl
import json
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from datetime import UTC, datetime

from compare import digest
from m5 import FREEZE as M5_FREEZE
from m5_compare import atomic_json
from m5_study import verify_freeze
from m5_train import config, run
from sailing import HERE, REPO

ROOT = REPO / "runs/ese3990/M5_sac_budget"
FREEZE = HERE / "configs/M5_sac_budget_freeze.json"
EVALUATOR = HERE / "m5_sac_budget_evaluate.py"


def verify():
    parent = verify_freeze(M5_FREEZE)
    frozen = json.loads(FREEZE.read_text())
    for name, expected in {**frozen["sources"], **frozen["baseline_digests"]}.items():
        if digest(REPO / name) != expected:
            raise ValueError(f"Frozen study input changed: {name}")
    expected = config("sac", "B", parent["start_box"], 40_000_000)
    baseline = dict(parent["configs"]["sac_B"])
    baseline.update(
        decisions=expected["decisions"], checkpoints=expected["checkpoints"]
    )
    if (
        expected != baseline
        or frozen["configs"]["sac_B_extended"] != expected
        or frozen["identity"] != parent["identity"]
        or frozen["training_seeds"] != parent["training_seeds"]
        or frozen["threshold"] != parent["threshold"]
    ):
        raise ValueError("Budget extension differs from the planned M5 comparison")
    return frozen


def run_dir(seed):
    return ROOT / "extended" / f"sac_B_s{seed}"


def train_seed(seed):
    frozen = verify()
    if seed not in frozen["training_seeds"]:
        raise ValueError(f"Seed outside frozen protocol: {seed}")
    run("sac", seed, run_dir(seed), frozen["configs"]["sac_B_extended"])


def train_subprocess(seed):
    out = run_dir(seed)
    if (out / "summary.json").is_file():
        return  # The evaluator validates the complete run and its budget.
    if out.exists():
        raise RuntimeError(f"Incomplete run retained at {out}, inspect before retry")
    with out.with_suffix(".log").open("a") as log:
        subprocess.run(
            [sys.executable, __file__, "train-seed", str(seed)],
            cwd=REPO,
            stdout=log,
            stderr=subprocess.STDOUT,
            check=True,
        )


def supervise():
    ROOT.mkdir(parents=True, exist_ok=True)
    with (ROOT / "study.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        frozen = verify()
        (ROOT / "extended").mkdir(exist_ok=True)
        state = dict(
            started_utc=datetime.now(UTC).isoformat(),
            pid=os.getpid(),
            training_freeze_sha256=digest(FREEZE),
        )

        def status(stage, **details):
            state.update(
                stage=stage, updated_utc=datetime.now(UTC).isoformat(), **details
            )
            atomic_json(ROOT / "status.json", state)

        try:
            status("training")
            with ThreadPoolExecutor(max_workers=len(frozen["training_seeds"])) as pool:
                list(pool.map(train_subprocess, frozen["training_seeds"]))
            status("audit_and_evaluation")
            subprocess.run(
                [sys.executable, str(EVALUATOR), "--jobs", "5"], cwd=REPO, check=True
            )
            status("complete")
        except Exception as exc:
            status("failed", error=str(exc))
            raise


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("run")
    commands.add_parser("verify")
    commands.add_parser("train-seed").add_argument("seed", type=int)
    args = parser.parse_args()
    if args.command == "run":
        supervise()
    elif args.command == "train-seed":
        train_seed(args.seed)
    else:
        verify()
        print(f"Verified {FREEZE} ({digest(FREEZE)})")
