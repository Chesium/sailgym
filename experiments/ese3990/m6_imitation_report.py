"""Verify and summarize the completed M6 handoff and imitation experiment."""

import numpy as np
from m6 import atomic_json, read
from m6_campaign import lines
from m6_imitation import BASE, ROOT, SEEDS
from m6_width_report import checked, counts
from sailing import REPO
from viz.episode_store import EpisodeStore


def panels(path):
    rows = checked(path)
    return {k: [r for r in rows if r["kind"] == k] for k in {r["kind"] for r in rows}}


def ids(rows):
    return {r["condition"] for r in rows if r["outcome"] == "finished"}


def validate_panel(rows, panel):
    assert [r["condition"] for r in rows] == [r["condition"]["id"] for r in panel]


def handoff_table(protocol):
    labels = ["early_early", "early_late", "late_early", "late_late"]
    results = {}
    stores = {k: EpisodeStore(ROOT / "handoff" / k / "store") for k in labels}
    original = ids(checked(ROOT / "handoff/early_early"))
    for label in labels:
        rows = checked(ROOT / "handoff" / label)
        validate_panel(rows, protocol["broad"])
        won = ids(rows)
        control = stores["_".join([label.split("_")[0]] * 2)]
        store = stores[label]
        for x, y in zip(store.index, control.index, strict=True):
            assert x["condition"] == y["condition"]
            end = protocol["opening_steps"]
            np.testing.assert_array_equal(
                store.load(x).actions[:end], control.load(y).actions[:end]
            )
        results[label] = dict(
            **counts(rows),
            retained=sorted(original & won),
            gained=sorted(won - original),
            lost=sorted(original - won),
            reached_switch=sum(
                e["decisions"] > protocol["opening_steps"] for e in store.index
            ),
        )
    return results


def seed_results(seed, protocol):
    out = ROOT / "pilot" / f"ppo_s{seed}"
    old = BASE / f"ppo_s{seed}"
    buf = read(out / "buffer.json")
    rows = checked(out)
    scores = panels(out / "evaluation")
    old_scores = panels(old / "evaluation")
    train = [r for r in rows if r["kind"] == "training" and r["outcome"] != "running"]
    # Match the buffer ledger to every finished training trajectory and timestep.
    ends = 10000
    ledger = []
    for ep in train:
        ends += ep["decisions"]
        if ep["outcome"] == "finished":
            ledger.append(dict(decisions=ends, length=ep["decisions"]))
    assert ledger == buf["wins"]
    anchors = ids(scores.get("anchor", []))
    if "anchor" in scores:
        validate_panel(scores["anchor"], protocol["broad"])
    qual_path = out / "qualifying.json"
    result = dict(
        buffer=buf,
        buffer_transitions=sum(e["length"] for e in ledger),
        training=counts(train),
        qualifying=read(qual_path) if qual_path.exists() else None,
        exact_replays=len(rows) + sum(map(len, scores.values())),
    )
    updates = lines(out / "imitation.jsonl")
    result["auxiliary_updates"] = sum(x["minibatches"] for x in updates)
    result["advantage_range"] = (
        [
            min(x["mean_positive_advantage"] for x in updates),
            max(x["mean_positive_advantage"] for x in updates),
        ]
        if updates
        else []
    )
    result["anchor_wins"] = sorted(anchors)
    for arm in ["baseline", "imitation"]:
        first = scores[arm + "_first_probe"]
        final = old_scores["broad"] if arm == "baseline" else scores["imitation_final"]
        fresh = scores[arm + "_fresh"]
        val = (old_scores if arm == "baseline" else scores)["original_validation"]
        for group, expected in [
            (first, "broad"),
            (final, "broad"),
            (fresh, "fresh"),
            (val, "validation"),
        ]:
            validate_panel(group, protocol[expected])
        won = ids(final)
        p = old if arm == "baseline" else out
        result[arm] = dict(
            first=counts(first),
            final=counts(final),
            fresh=counts(fresh),
            validation=counts(val),
            first_retained=sorted(ids(first) & won),
            anchor_retained=sorted(anchors & won),
            final_wins=sorted(won),
            probes=[x["finishes"] for x in lines(p / "probes.jsonl")],
        )
    if (out / "transfer").exists():
        ts = checked(out / "transfer")
        result["exact_replays"] += len(ts)
        result["transfer"] = {
            f"{g}_{b}": counts(
                [r for r in ts if r["geometry"] == g and r["board"] == b]
            )
            for g in ("direct", "switch")
            for b in (-1, 1)
        }
    return result


def report():
    status = read(ROOT / "status.json")
    assert status["phase"] == "complete"
    freeze = read(ROOT / "protocol.json")
    handoff = handoff_table(freeze)
    seeds = {seed: seed_results(seed, freeze) for seed in SEEDS}
    decision = read(ROOT / "decision.json")
    n = sum(v["n"] for v in handoff.values()) + sum(
        v["exact_replays"] for v in seeds.values()
    )
    output = REPO / "docs/ese3990"
    atomic_json(
        output / "M6_imitation_results.json",
        dict(
            wall_s=status["elapsed_s"],
            exact_replays=n,
            handoff=handoff,
            seeds=seeds,
            decision=decision,
        ),
    )
    text = [
        "# M6 · Opening-policy handoff and PPO self-imitation",
        "",
        f"The campaign completed in {status['elapsed_s'] / 60:.1f} minutes. Exact "
        f"replay covered {n:,} new episodes. The policy remained frozen during "
        "every evaluation. [Protocol](../../runs/ese3990/M6/imitation/protocol.json)"
        " · [Results by seed and condition](M6_imitation_results.json)",
        "",
        "## Two-second handoff on seed 100",
        "",
        "| Opening → remainder | Finishes / 80 | Retained early wins / 16 | "
        "Other finishes / 64 |",
        "|---|---:|---:|---:|",
    ]
    for k, v in handoff.items():
        text.append(
            f"| {' → '.join(k.split('_'))} | {v['wins']} | {len(v['retained'])} | "
            f"{len(v['gained'])} |"
        )
    text += [
        "",
        "Early and late refer to 77,991 and 400,000 decisions. The controls "
        "matched the archived actions at every step. The first 40 actions of "
        "each hybrid matched its opening control on every condition.",
        "",
        "## Broad starts, 400k raw decisions per seed",
        "",
        "| Seed | Policy | Common anchor retained | First probe retained | "
        "Final fixed / 80 | Final fresh / 80 | Fresh promotion wins per 20 | "
        "40 m validation / 20 |",
        "|---:|---|---:|---:|---:|---:|---|---:|",
    ]
    for seed, s in seeds.items():
        for arm in ("baseline", "imitation"):
            q = s[arm]
            retained = f"{len(q['anchor_retained'])}/{len(s['anchor_wins'])}"
            early = f"{len(q['first_retained'])}/{q['first']['wins']}"
            probes = ", ".join(map(str, q["probes"]))
            text.append(
                f"| {seed} | {arm} | {retained} | {early} | {q['final']['wins']} | "
                f"{q['fresh']['wins']} | {probes} | {q['validation']['wins']} |"
            )
    text += [
        "",
        "Each common anchor was saved immediately before the first update with "
        "self-imitation. Actions through that rollout matched ordinary PPO exactly. "
        "A denominator of zero means retention could not be measured for that "
        "snapshot. The first-probe comparison uses each arm's own earliest "
        "promotion snapshot. All retention counts use identical start conditions.",
        "",
        "| Seed | First training finish at decision | First affected update after "
        "decision | Successful episodes | Replay transitions | PPO minibatches "
        "with auxiliary loss |",
        "|---:|---:|---:|---:|---:|---:|",
    ]
    for seed, s in seeds.items():
        b = s["buffer"]
        fst = b["wins"][0]["decisions"] if b["wins"] else "–"
        text.append(
            f"| {seed} | {fst} | {b['anchor'] or '–'} | {len(b['wins'])} | "
            f"{s['buffer_transitions']} | {s['auxiliary_updates']} |"
        )
    text += [
        "",
        "The buffer ledger matched every completed online training success. Each "
        "PPO minibatch after activation added an actor loss with coefficient 0.1. "
        "The detached weight was the positive part of discounted return-to-go "
        "minus current value. Sampling used a separate random stream. The ordinary "
        "PPO controls were the previously completed matched broad-start runs.",
        "",
        f"{len(decision['qualified'])}/3 seeds qualified on two successive fresh "
        "promotion probes. Pooled common-anchor finishes retained at 400k were "
        f"{decision['retained']['imitation']} for self-imitation and "
        f"{decision['retained']['baseline']} for ordinary PPO. "
        + (
            "The qualifying snapshots ran the 3–7 m transfer test."
            if decision["run_transfer"]
            else "The 3–7 m transfer gate stayed closed."
        ),
        "",
    ]
    text += ["## Reading across seeds", ""]
    for arm in ("baseline", "imitation"):
        fresh = sum(s[arm]["fresh"]["wins"] for s in seeds.values())
        val = sum(s[arm]["validation"]["wins"] for s in seeds.values())
        text.append(
            f"- {arm.capitalize()} finished {fresh}/240 new near-gate evaluations "
            f"and {val}/60 original-start validations."
        )
    text.extend(
        [
            "",
            "Counts pool three matched seeds evaluated on the same conditions. "
            "The per-seed rows show the variation across policies.",
            "",
        ]
    )
    for seed, s in seeds.items():
        for k, v in s.get("transfer", {}).items():
            text.append(
                f"- Seed {seed}, {k}: {v['wins']}/{v['n']} finishes, "
                f"{v['settled_wins']} with a settled tack."
            )
    text.extend(
        [
            "[Campaign runner](../../experiments/ese3990/m6_imitation.py) · "
            "[Episode stores and checkpoints](../../runs/ese3990/M6/imitation)",
            "",
        ]
    )
    (output / "M6_imitation_report.md").write_text("\n".join(text))


if __name__ == "__main__":
    report()
