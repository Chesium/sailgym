"""Generate verified side-specific M6 mirror results from the completed campaign."""

from m6 import atomic_json, read
from m6_campaign import lines
from m6_imitation_report import ids, panels, validate_panel
from m6_mirror import CONTROL, PANELS, ROOT, SEEDS
from m6_width_report import checked, counts
from sailing import REPO


def sides(rows):
    return {
        side: counts([r for r in rows if (r["m5_start_xy"][0] < 0) == west])
        for side, west in [("west", True), ("east", False)]
    }


def seed_report(seed, protocol):
    out = ROOT / "pilot" / f"ppo_s{seed}"
    control = CONTROL / "pilot" / f"ppo_s{seed}"
    shadow = ROOT / "reflections" / f"ppo_s{seed}"
    cost = read(shadow / "cost.json")
    online = checked(out)
    copies = checked(shadow)
    ev = panels(out / "evaluation")
    archived = panels(control / "evaluation")
    total = len(online) + len(copies) + sum(map(len, ev.values()))
    source_wins = [
        r for r in online if r["kind"] == "training" and r["outcome"] == "finished"
    ]
    assert len(source_wins) == len(copies) == cost["real_successes"]
    assert sum(x["decisions"] for x in copies) == cost["extra_decisions"]
    result = dict(
        cost=cost,
        training_success=sides(source_wins),
        reflections=sides(copies),
        mirror_probes=[p["finishes"] for p in lines(out / "probes.jsonl")],
        control_probes=[p["finishes"] for p in lines(control / "probes.jsonl")],
    )
    for name, old_kind in [
        ("paired", "imitation_final"),
        ("previous", "imitation_fresh"),
        ("new", None),
    ]:
        left = archived[old_kind] if old_kind else ev["control_new"]
        right = ev["mirror_" + name]
        for rows in (left, right):
            validate_panel(rows, protocol[name])
        winners = [ids(rows) for rows in (left, right)]
        entry = dict(control=sides(left), mirror=sides(right))
        for side, idx in (("west", -1), ("east", 1)):
            group = {r["condition"] for r in left if r["m5_start_xy"][0] * idx > 0}
            assert len(group) == 40
            a, b = [w & group for w in winners]
            entry[side] = dict(
                retained=sorted(a & b), gained=sorted(b - a), lost=sorted(a - b)
            )
        if name == "paired":
            pairs = list(zip(left[::2], left[1::2], strict=True))
            for a, b in pairs:
                assert a["m5_start_xy"][0] == -b["m5_start_xy"][0]
            entry["both"] = [
                sum(a["condition"] in w and b["condition"] in w for a, b in pairs)
                for w in winners
            ]
        result[name] = entry
    result["validation"] = [
        counts(rows)["wins"]
        for rows in (archived["original_validation"], ev["mirror_validation"])
    ]
    if (out / "transfer").exists():
        transfer = checked(out / "transfer")
        total += len(transfer)
        result["transfer"] = {
            f"{g}_{b}": counts(
                [r for r in transfer if r["geometry"] == g and r["board"] == b]
            )
            for g in ("direct", "switch")
            for b in (-1, 1)
        }
    result["new_replays"] = total
    return result


def report():
    status = read(ROOT / "status.json")
    assert status["phase"] == "complete"
    protocol = read(ROOT / "protocol.json")
    data = {s: seed_report(s, protocol) for s in SEEDS}
    n = sum(r["new_replays"] for r in data.values())
    out = REPO / "docs/ese3990"
    atomic_json(
        out / "M6_mirror_results.json", dict(status=status, seeds=data, exact_replays=n)
    )
    text = [
        "# M6 · Simulator-verified mirrored self-imitation",
        "",
        f"Three matched PPO seeds ran for 400,000 online decisions each in "
        f"{status['elapsed_s'] / 60:.1f} minutes wall time. All {n:,} new episode "
        "records replayed exactly. Both arms used broad starts at rest, 1–3 m "
        "below the gate, the same PPO optimizer, and self-imitation weight 0.1. "
        "Each mirrored arm used only its own online successes to generate extra "
        "attempts. [Frozen protocol](../../runs/ese3990/M6/mirror/protocol.json) · "
        "[Machine-readable results](M6_mirror_results.json)",
        "",
        "## Fixed-final finishes by starting side",
        "",
        "Each cell gives finishes out of 40. Control is the ordinary self-imitation "
        "checkpoint on the same seed and starts. The previous panel is where the "
        "east–west discrepancy was identified. The new panel was frozen for this "
        "experiment. The paired panel contains 40 reflected start pairs.",
        "",
        "| Panel | Seed | West control | West mirror | East control | East mirror |",
        "|---|---:|---:|---:|---:|---:|",
    ]
    for name in PANELS:
        for seed, r in data.items():
            v = r[name]
            vals = [
                v[arm][side]["wins"]
                for side in ("west", "east")
                for arm in ("control", "mirror")
            ]
            text.append(f"| {name} | {seed} | " + " | ".join(map(str, vals)) + " |")
    text += [
        "",
        "## Extra simulation and promotion",
        "",
        "| Seed | Online finishes | Reflected finishes / attempts | Extra decisions "
        "| Extra rows admitted | Pairs finished on both sides, control → mirror "
        "| Mirror promotion finishes per 20 | 40 m finishes, control → mirror / 20 |",
        "|---:|---:|---:|---:|---:|---:|---|---:|",
    ]
    for seed, r in data.items():
        c = r["cost"]
        a, b = r["paired"]["both"]
        v, w = r["validation"]
        probes = ", ".join(map(str, r["mirror_probes"]))
        text.append(
            f"| {seed} | {c['real_successes']} | {c['mirror_finishes']}/"
            f"{c['real_successes']} | {c['extra_decisions']} | {c['mirror_rows']} "
            f"| {a} → {b} | {probes} | {v} → {w} |"
        )
    text += [
        "",
        f"{len(status['qualified'])}/3 seeds passed two successive fresh probes "
        "with at least 6/20 finishes each above the fixed controls. "
        + (
            "The 3–7 m paired transfer was evaluated."
            if status["transfer"]
            else "The 3–7 m paired transfer gate stayed closed."
        ),
        "",
        "The numerical results identify the individual starts gained, retained "
        "and lost on each side. Reproduce this report after training with "
        "`env -u PYTHONPATH .venv/bin/python experiments/ese3990/m6_mirror_report.py`. "
        "[Training and replay code](../../experiments/ese3990/m6_mirror.py) · "
        "[Run records](../../runs/ese3990/M6/mirror)",
        "",
    ]
    (out / "M6_mirror_report.md").write_text("\n".join(text))


if __name__ == "__main__":
    report()
